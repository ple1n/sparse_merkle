#![allow(clippy::while_let_loop)]
#![allow(clippy::single_match)]
#![allow(unused)]
#![allow(clippy::type_complexity)]

use std::{collections::BTreeSet, default};

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
    Directed, algo::min_spanning_tree, graph::NodeIndex, graphmap, stable_graph::StableGraph,
    visit::IntoNodeReferences,
};
use smt::wot::{self, EdgeRuntime, Node};

pub struct BasicApp {
    g: Option<Graph<Node<VisualData>, EdgeRuntime<VisualData>, Directed, u32, NodeShape>>,
    pick_root: bool,
    pick_owned: bool,
    rx: Receiver<Event>,
    sx: Sender<Event>,
    reset: bool,
}

type LayoutState = LayoutForce;
type Layout = LayoutForce;

#[derive(Debug, Default, Hash, PartialEq, PartialOrd, Eq, Ord, Clone, Copy)]
pub struct VisualData {
    selected: bool,
    mark_owned: bool,
    mark_root: bool,
    /// Virtual node, for GUI purpose
    virt: bool,
}

impl BasicApp {
    fn new(_: &CreationContext<'_>) -> Self {
        let (sx, rx) = crossbeam::channel::unbounded();
        Self {
            g: Some(rand_view()),
            pick_owned: false,
            pick_root: false,
            sx,
            rx,
            reset: false,
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
        data: VisualData {
            selected: false,
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
                    selected: false,
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
                if n.payload().data.virt {
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

impl App for BasicApp {
    fn update(&mut self, ctx: &Context, f: &mut eframe::Frame) {
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
                            let (n, p) = g.node_mut(NodeIndex::new(node.id)).unwrap();
                            if self.pick_root {
                                n.payload_mut().data.mark_root = true;
                            }
                            n.payload_mut().data.selected = true;
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
        Box::new(|cc| Ok(Box::new(BasicApp::new(cc)))),
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
            data: A::default(),
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
