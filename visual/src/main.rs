#![allow(clippy::style)]
#![allow(unused)]
#![allow(clippy::complexity)]

use std::{
    any,
    collections::{BTreeMap, BTreeSet},
    default,
};

use crossbeam::channel::{self, Receiver, Sender};
use eframe::{App, CreationContext, NativeOptions, run_native};
use egui::{Button, Context, Id, emath};
use egui_graphs::{
    DefaultEdgeShape, DefaultGraphView, Graph, GraphView, LayoutForce,
    events::Event,
    graph::{FEdge, FNode, ForceGraphType},
    new_from_raw, to_graph_custom,
};
use fdg::{
    Force, ForceGraph,
    fruchterman_reingold::{FruchtermanReingold, FruchtermanReingoldConfiguration},
    nalgebra::OPoint,
    simple::Center,
};
use petgraph::{
    Directed,
    algo::min_spanning_tree,
    graph::{EdgeIndex, NodeIndex},
    graphmap,
    stable_graph::StableGraph,
    visit::{EdgeRef, IntoEdgeReferences, IntoEdgesDirected, IntoNodeReferences},
};
use smt::{
    smt::Proof,
    wot::{self, Conv, EdgeRuntime, MapGraph, Node},
};

#[derive(Clone)]
pub struct VisualNode {
    node: Node,
    mark_root: bool,
    mark_owned: bool,
    mapped: bool,
}

#[derive(Clone)]
pub struct VisualEdge {
    edge: EdgeRuntime,
    mapped: bool,
}

impl Default for VisualEdge {
    fn default() -> Self {
        VisualEdge {
            edge: EdgeRuntime { fraction: 0 },
            mapped: false,
        }
    }
}
impl Default for VisualNode {
    fn default() -> Self {
        VisualNode {
            node: Node {
                score: 0,
                proof: None,
            },
            mark_root: false,
            mark_owned: false,
            mapped: false,
        }
    }
}

pub struct VisualGrapher;

impl MapGraph for VisualGrapher {
    type IxE = EdgeIndex;
    type IxN = NodeIndex;
    type S = TyGraph;
    fn map_node(&mut self, sg: &mut Self::S, node: Self::IxN, new: Option<Node>) -> bool {
        let is = sg[node].0.payload_mut().mapped;
        sg[node].0.payload_mut().mapped = true;
        is
    }
    fn map_edge(&mut self, sg: &mut Self::S, edge: Self::IxE, new: Option<EdgeRuntime>) -> bool {
        let is = sg[edge].payload_mut().mapped;
        sg[edge].payload_mut().mapped = true;
        is
    }
}

#[derive(Default)]
pub struct VisualConv;

impl Conv for VisualConv {
    type Edge = FEdge<VisualNode, VisualEdge, Directed, u32, node::NodeShape>;
    type Node = FNode<VisualNode, VisualEdge, Directed, u32, node::NodeShape>;
    fn edge_mut(edge: &mut Self::Edge) -> &mut EdgeRuntime {
        &mut edge.payload_mut().edge
    }
    fn edge_ref(edge: &Self::Edge) -> &EdgeRuntime {
        &edge.payload().edge
    }
    fn node_mut(node: &mut Self::Node) -> &mut Node {
        &mut node.0.props.payload.node
    }
    fn node_ref(node: &Self::Node) -> &Node {
        &node.0.props.payload.node
    }
}

pub type TyGraph = ForceGraphType<VisualNode, VisualEdge, Directed, u32, NodeShape>;
pub type TyGraphUI = Graph<VisualNode, VisualEdge, Directed, u32, NodeShape>;

pub struct AppZK {
    g: Option<TyGraphUI>,
    pick_root: bool,
    pick_owned: bool,
    rx: Receiver<Event>,
    sx: Sender<Event>,
    reset: bool,
    root_nodes: BTreeSet<NI>,
    owned_nodes: BTreeSet<NI>,
}

impl AppZK {
    pub fn clear_selection(&mut self) {
        self.root_nodes.clear();
        self.owned_nodes.clear();
    }
    pub fn compute(&mut self) -> Result<(), anyhow::Error> {
        println!("compute");
        use smt::wot::*;

        if let Some(g) = self.g.as_mut() {
            let gx = g.g_mut();
            let mut proving = ProofWeb::new(gx, VisualConv);
            proving.public.methods = Methods::Web {
                roots: Default::default(),
            };
            let roots = match &mut proving.public.methods {
                Methods::Web { roots } => roots,
                _ => unreachable!(),
            };

            construct(proving, MockVerify, &mut VisualGrapher);
        }
        Ok(())
    }
}

type NI = NodeIndex<u32>;

type LayoutState = LayoutForce;
type Layout = LayoutForce;

#[derive(Debug, Default, Hash, PartialEq, PartialOrd, Eq, Ord, Clone, Copy)]
pub struct VisualData {
    mark_owned: bool,
    mark_root: bool,
    /// Virtual node, for GUI purpose
    virt: bool,
}

impl AppZK {
    fn new(_: &CreationContext<'_>) -> Self {
        let (sx, rx) = crossbeam::channel::unbounded();
        Self {
            g: Some(rand_view()),
            pick_owned: false,
            pick_root: false,
            sx,
            rx,
            reset: false,
            owned_nodes: Default::default(),
            root_nodes: Default::default(),
        }
    }
}

pub fn rand_view() -> TyGraphUI {
    let mut sg: StableGraph<_, _> = gen_graph();
    println!("gen new graph {}", sg.node_count());
    let mut islands = BTreeSet::new();
    for (ni, no) in sg.node_references() {
        if sg
            .neighbors_directed(ni, petgraph::Direction::Incoming)
            .count()
            == 0
        {
            islands.insert(ni);
        }
    }
    let rt = sg.add_node(Default::default());
    for n in islands {
        sg.add_edge(rt, n, Default::default());
    }
    let g: TyGraphUI = new_from_raw(&sg, &mut |_n: &mut _| {}, &mut |_e: &mut _| {});

    println!("num {} {}", g.node_count(), g.edge_count());
    g
}
use egui_graphs::{SettingsInteraction, SettingsNavigation, SettingsStyle};

use crate::node::NodeShape;

impl App for AppZK {
    fn update(&mut self, ctx: &Context, f: &mut eframe::Frame) {
        ctx.options_mut(|op| op.scroll_zoom_speed = 10.);
        egui::SidePanel::new(egui::panel::Side::Right, Id::new("controls")).show(ctx, |ui| {
            ui.add_space(20.);
            if ui.selectable_label(self.pick_root, "pick root").clicked() {
                self.pick_root = true;
                self.pick_owned = false;
            }
            if ui.selectable_label(self.pick_owned, "pick owned").clicked() {
                self.pick_root = false;
                self.pick_owned = true;
            }
            ui.add_space(20.);

            if ui.button("randomize").clicked() {
                let g = rand_view();
                self.reset = true;
                self.g = Some(g);
                self.clear_selection();
            }
            if ui.button("eval").clicked() {
                let x = self.compute();
                println!("eval {:?}", x);
            }
            if let Some(g) = &self.g {
                ui.label(format!("hovered {:?}", g.meta.hovered));
            }
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            let interaction_settings = &SettingsInteraction::new()
                .with_dragging_enabled(true)
                .with_node_clicking_enabled(true)
                .with_node_selection_enabled(true)
                .with_node_selection_multi_enabled(true)
                .with_edge_clicking_enabled(true)
                .with_edge_selection_enabled(true)
                .with_edge_selection_multi_enabled(true);
            let style_settings = &SettingsStyle::new().with_labels_always(true);
            let navigation_settings = &SettingsNavigation::new()
                .with_fit_to_screen_enabled(false)
                .with_zoom_and_pan_enabled(true)
                .with_zoom_speed(0.04);

            if let Some(g) = &mut self.g {
                if self.reset {
                    ui.data_mut(|data| {
                        data.clear();
                    });
                    self.reset = false;
                }
                let mut gv = GraphView::<
                    VisualNode,
                    VisualEdge,
                    Directed,
                    u32,
                    NodeShape,
                    _,
                    LayoutState,
                    Layout,
                >::new(g)
                .with_styles(style_settings)
                .with_interactions(interaction_settings)
                .with_navigations(navigation_settings)
                .with_events(&self.sx);

                ui.add(&mut gv);
            };
        });
        if let Some(g) = &mut self.g {
            loop {
                if let Ok(ev) = self.rx.try_recv() {
                    match ev {
                        Event::NodeSelect(node) => {
                            let ni = NodeIndex::new(node.id);
                            let (n, p) = g.node_mut(ni).unwrap();
                            let mut mark = |map: &mut BTreeSet<NI>, var: &mut bool| {
                                *var = !*var;
                                if *var {
                                    map.insert(ni);
                                } else {
                                    map.remove(&ni);
                                }
                                dbg!(&map);
                            };
                            if self.pick_root {
                                let p = &mut n.payload_mut().mark_root;
                                mark(&mut self.root_nodes, p);
                            }
                            if self.pick_owned {
                                let p = &mut n.payload_mut().mark_owned;
                                mark(&mut self.owned_nodes, p);
                            }

                            use smt::wot::construct;
                        }
                        _ => {
                            // dbg!(&ev);
                        }
                    }
                } else {
                    break;
                }
            }
        }
    }
}

fn main() {
    run_native(
        "egui_graphs_basic_demo",
        NativeOptions::default(),
        Box::new(|cc| Ok(Box::new(AppZK::new(cc)))),
    )
    .unwrap();
}

fn gen_graph() -> StableGraph<VisualNode, VisualEdge> {
    use graphalgs::generate::random_weighted_digraph;
    use rand::distributions::uniform::UniformSampler;
    use smt::wot::notzk::WeightSampler;

    let node_num = 30;
    let nedge = (1.5 * node_num as f32) as usize;
    println!("node {} edge {}", node_num, nedge);

    let g = random_weighted_digraph(
        node_num,
        nedge,
        EdgeRuntime { fraction: 1 },
        EdgeRuntime { fraction: 100 },
    )
    .unwrap();
    let mut sg: StableGraph<VisualNode, VisualEdge> = StableGraph::new();

    for _ix in 0..node_num {
        sg.add_node(Default::default());
    }

    for ((n1, n2), e) in g {
        let n1 = (n1 as u32).into();
        let n2 = (n2 as u32).into();
        sg.add_edge(
            n1,
            n2,
            VisualEdge {
                edge: e,
                ..Default::default()
            },
        );
    }

    sg
}

mod node;
