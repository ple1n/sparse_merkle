use eframe::{App, CreationContext, NativeOptions, run_native};
use egui::Context;
use egui_graphs::{
    DefaultGraphView, Graph, GraphView, LayoutHierarchical, LayoutStateHierarchical,
};
use petgraph::{Directed, stable_graph::StableGraph};
use smt::wot::{self, Node, WeightRuntime};

pub struct BasicApp {
    g: Graph<Node, WeightRuntime, Directed, u32>,
}

impl BasicApp {
    fn new(_: &CreationContext<'_>) -> Self {
        let g = gen_graph();
        Self { g: Graph::from(&g) }
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
                    Node,
                    WeightRuntime,
                    Directed,
                    u32,
                    _,
                    _,
                    LayoutStateHierarchical,
                    LayoutHierarchical,
                >::new(&mut self.g)
                .with_styles(style_settings)
                .with_interactions(interaction_settings)
                .with_navigations(navigation_settings),
            );
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

fn gen_graph() -> StableGraph<wot::Node, WeightRuntime> {
    use graphalgs::generate::random_weighted_digraph;
    use rand::distributions::uniform::UniformSampler;
    use smt::wot::notzk::WeightSampler;

    let node_num = 30;
    let nedge = (1.5 * node_num as f32) as usize;
    println!("{} {}", node_num, nedge);

    let g = random_weighted_digraph(
        node_num,
        nedge,
        WeightRuntime { fraction: 1 },
        WeightRuntime { fraction: 100 },
    )
    .unwrap();
    let mut sg: StableGraph<wot::Node, WeightRuntime> = StableGraph::new();
    for _ix in 0..node_num {
        sg.add_node(wot::Node {
            score: 0,
            proof: None,
        });
    }

    for ((n1, n2), e) in g {
        let n1 = (n1 as u32).into();
        let n2 = (n2 as u32).into();
        sg.add_edge(n1, n2, e);
    }

    sg
}
