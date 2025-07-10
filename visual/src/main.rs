use std::default;

use crossbeam::channel::{self, Receiver, Sender};
use eframe::{App, CreationContext, NativeOptions, run_native};
use egui::{Button, Context, Id};
use egui_graphs::{
    DefaultGraphView, Graph, GraphView, LayoutForceDirected, LayoutHierarchical,
    LayoutStateForceDirected, LayoutStateHierarchical, events::Event,
};
use petgraph::{Directed, graphmap, stable_graph::StableGraph, visit::IntoNodeReferences};
use smt::wot::{self, EdgeRuntime, Node};

pub struct BasicApp {
    g: Graph<Node<VisualData>, EdgeRuntime<VisualData>, Directed, u32>,
    pick_root: bool,
    pick_owned: bool,
    rx: Receiver<Event>,
    sx: Sender<Event>,
}

#[derive(Debug, Default, Hash, PartialEq, PartialOrd, Eq, Ord, Clone, Copy)]
pub struct VisualData {
    selected: bool,
}

impl BasicApp {
    fn new(_: &CreationContext<'_>) -> Self {
        let g = gen_graph();
        let (sx, rx) = crossbeam::channel::unbounded();
        Self {
            g: Graph::from(&g),
            pick_owned: false,
            pick_root: false,
            sx,
            rx,
        }
    }
}
use egui_graphs::{SettingsInteraction, SettingsNavigation, SettingsStyle};

impl App for BasicApp {
    fn update(&mut self, ctx: &Context, _: &mut eframe::Frame) {
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

            ui.add(
                &mut GraphView::<
                    Node<VisualData>,
                    EdgeRuntime<VisualData>,
                    Directed,
                    u32,
                    _,
                    _,
                    LayoutStateHierarchical,
                    LayoutHierarchical,
                >::new(&mut self.g)
                .with_styles(style_settings)
                .with_interactions(interaction_settings)
                .with_navigations(navigation_settings), // .with_events(&self.sx),
            );
        });
        // for ev in self.rx {
        //     match ev {
        //         Event::NodeSelect(node) => self.g[node],
        //     }
        // }
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
        });
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
    println!("{} {}", node_num, nedge);

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
