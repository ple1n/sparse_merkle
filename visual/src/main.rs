#![allow(clippy::style)]
#![allow(unused)]
#![allow(clippy::complexity)]

use std::{
    collections::{BTreeMap, BTreeSet},
    default,
};

use crossbeam::channel::{self, Receiver, Sender};
use eframe::{App, CreationContext, NativeOptions, run_native};
use egui::{Button, Context, Id, emath};
use egui_graphs::{
    DefaultEdgeShape, DefaultGraphView, Graph, GraphView, LayoutForce, events::Event, new_from_raw,
    to_graph_custom,
};
use fdg::{
    Force, ForceGraph,
    fruchterman_reingold::{FruchtermanReingold, FruchtermanReingoldConfiguration},
    simple::Center,
};
use petgraph::{
    Directed,
    algo::min_spanning_tree,
    graph::NodeIndex,
    graphmap,
    stable_graph::StableGraph,
    visit::{EdgeRef, IntoEdgeReferences, IntoEdgesDirected, IntoNodeReferences},
};
use smt::wot::{self, EdgeRuntime, Node};

pub struct AppZK {
    g: Option<Graph<Node<VisualData>, EdgeRuntime<VisualData>, Directed, u32, NodeShape>>,
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
    pub fn compute(&mut self) {
        println!("compute");
        use smt::wot::*;
        let mut rweb: StableGraph<Node<VisualData>, EdgeRuntime<VisualData>> =
            RuntimeWeb::<VisualData>::new();
        if let Some(g) = self.g.as_mut() {
            let mut map = BTreeMap::new();
            for (x, (n, p)) in g.g().node_references() {
                let mut node = n.payload().to_owned();
                node.add.ix = x.index() as u32;
                let new = rweb.add_node(node);
                map.insert(x, new);
            }
            for e in g.g().edge_references() {
                rweb.add_edge(
                    *map.get(&e.source()).unwrap(),
                    *map.get(&e.target()).unwrap(),
                    e.weight().payload().clone(),
                );
            }

            let mut proving = ProofWeb::default();
            proving.web = rweb;
            proving.public.methods = Methods::Web {
                roots: Default::default(),
            };
            let roots = match &mut proving.public.methods {
                Methods::Web { roots } => roots,
                _ => unreachable!(),
            };

            for (x, n) in proving.web.node_references() {
                if n.add.mark_owned {
                    proving.owned.insert(x.index() as u32, IdentityPub::Mock);
                }
                if n.add.mark_root {
                    roots.insert(x.index() as u32, 100);
                }
            }
            let pruned = construct(proving, MockVerify);
            println!(
                "pruned {} {}",
                pruned.web.node_count(),
                pruned.web.edge_count()
            );
            for (x, n) in pruned.web.node_references() {
                *g.g_mut().node_weight_mut(x).unwrap().0.payload_mut() = n.clone();
            }
            for e in pruned.web.edge_references() {
                *(g.edge_mut(e.id()).unwrap().payload_mut()) = e.weight().clone();
            }
        }
    }
}

type NI = NodeIndex<u32>;

type LayoutState = LayoutForce;
type Layout = LayoutForce;

#[derive(Debug, Default, Hash, PartialEq, PartialOrd, Eq, Ord, Clone, Copy)]
pub struct VisualData {
    mark_owned: bool,
    mark_root: bool,
    /// Ix in GUI
    ix: u32,
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

pub fn rand_view() -> Graph<Node<VisualData>, EdgeRuntime<VisualData>, Directed, u32, NodeShape> {
    let mut sg: StableGraph<Node<VisualData>, EdgeRuntime<VisualData>> = gen_graph();
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
    let rt = sg.add_node(Node {
        score: 0,
        proof: None,
        add: VisualData {
            virt: true,
            ..Default::default()
        },
    });
    for n in islands {
        sg.add_edge(
            rt,
            n,
            EdgeRuntime {
                fraction: 0,
                data: VisualData {
                    virt: true,
                    ..Default::default()
                },
            },
        );
    }
    let g: Graph<Node<VisualData>, EdgeRuntime<VisualData>, Directed, u32, NodeShape> =
        new_from_raw(
            &sg,
            &mut |n: &mut _| {
                if n.payload().add.virt {
                    n.props.hidden = true
                }
            },
            &mut |e: &mut _| {},
        );

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
                self.compute();
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
                    Node<VisualData>,
                    EdgeRuntime<VisualData>,
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
                                let p = &mut n.payload_mut().add.mark_root;
                                mark(&mut self.root_nodes, p);
                            }
                            if self.pick_owned {
                                let p = &mut n.payload_mut().add.mark_owned;
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

fn gen_graph<A: Default, B: Default + Clone + PartialOrd + Copy>()
-> StableGraph<wot::Node<A>, EdgeRuntime<B>> {
    use graphalgs::generate::random_weighted_digraph;
    use rand::distributions::uniform::UniformSampler;
    use smt::wot::notzk::WeightSampler;

    let node_num = 30;
    let nedge = (1.5 * node_num as f32) as usize;
    println!("node {} edge {}", node_num, nedge);

    let g = random_weighted_digraph(
        node_num,
        nedge,
        EdgeRuntime {
            fraction: 1,
            data: B::default(),
        },
        EdgeRuntime {
            fraction: 100,
            data: B::default(),
        },
    )
    .unwrap();
    let mut sg: StableGraph<wot::Node<A>, EdgeRuntime<B>> = StableGraph::new();

    for _ix in 0..node_num {
        sg.add_node(wot::Node {
            score: 0,
            proof: None,
            add: A::default(),
        });
    }

    for ((n1, n2), e) in g {
        let n1 = (n1 as u32).into();
        let n2 = (n2 as u32).into();
        sg.add_edge(n1, n2, e);
    }

    sg
}

mod node;
