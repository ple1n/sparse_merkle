use egui::{
    Color32, FontFamily, FontId, Pos2, Shape, Stroke, Vec2,
    epaint::{CircleShape, TextShape},
};
use petgraph::{EdgeType, stable_graph::IndexType};
use smt::wot;

use egui_graphs::{DisplayNode, DrawContext, NodeProps, metadata::GraphElement};

use crate::{VisualData, VisualNode};

/// This is the default node shape which is used to display nodes in the graph.
///
/// You can use this implementation as an example for implementing your own custom node shapes.
#[derive(Clone)]
pub struct AppNodeShape {
    pub pos: Pos2,

    pub selected: bool,
    pub dragged: bool,
    pub color: Option<Color32>,

    pub label_text: String,

    pub radius: f32,
    pub hidden: bool,
    pub props: NodeProps<NV>,
}

impl From<NodeProps<NV>> for AppNodeShape {
    fn from(node_props: NodeProps<NV>) -> Self {
        AppNodeShape {
            pos: node_props.location(),
            selected: node_props.selected,
            dragged: node_props.dragged,
            label_text: node_props.label.to_string(),
            color: node_props.color(),
            hidden: node_props.hidden,
            radius: 5.0,
            props: node_props,
        }
    }
}

type NV = VisualNode;

impl<E: Clone, Ty: EdgeType, Ix: IndexType> DisplayNode<NV, E, Ty, Ix> for AppNodeShape {
    fn is_inside(&self, pos: Pos2) -> bool {
        is_inside_circle(self.pos, self.radius, pos)
    }

    fn closest_boundary_point(&self, dir: Vec2) -> Pos2 {
        closest_point_on_circle(self.pos, self.radius, dir)
    }

    fn shapes(&mut self, ctx: &DrawContext, state: &NodeProps<NV>) -> Vec<Shape> {
        let mut res = Vec::with_capacity(2);

        // if self.hidden {
        //     return res;
        // }

        let is_interacted = self.selected || self.dragged;

        let style = if is_interacted {
            ctx.ctx.style().visuals.widgets.active
        } else {
            ctx.ctx.style().visuals.widgets.inactive
        };

        let mut color = if let Some(c) = self.color {
            c
        } else {
            style.fg_stroke.color
        };

        if self.props.payload.mark_root {
            color = color.blend(Color32::ORANGE.gamma_multiply(0.9))
        }
        if self.props.payload.mark_owned {
            color = color.blend(Color32::LIGHT_GREEN.gamma_multiply(0.9))
        }

        if self.hidden {
            color = color.blend(Color32::WHITE.gamma_multiply(0.1));
        }

        let circle_center = ctx.meta.canvas_to_screen_pos(self.pos);
        let circle_radius = ctx.meta.canvas_to_screen_size(self.radius);
        let mut st = Stroke::default();
        if let Some(GraphElement::Node(ix)) = ctx.meta.hovered {
            if let Some(i2) = self.props.index {
                if ix == i2 {
                    // color = color.blend(Color32::LIGHT_GREEN.gamma_multiply(0.5));
                    st.width = 10.;
                    st.color = Color32::WHITE.gamma_multiply(0.4);
                }
            }
        }

        let circle_shape = CircleShape {
            center: circle_center,
            radius: circle_radius,
            fill: color,
            stroke: st,
        };

        res.push(circle_shape.into());

        let mut label_visible = ctx.style.labels_always || self.selected || self.dragged;

        if state.payload.mapped {
            label_visible = true;
            self.label_text = format!("{}", state.payload.node.score);
        }

        if !label_visible {
            return res;
        }

        let galley = ctx.ctx.fonts(|f| {
            f.layout_no_wrap(
                self.label_text.clone(),
                FontId::new(circle_radius, FontFamily::Monospace),
                color,
            )
        });

        // display label centered over the circle
        let label_pos = Pos2::new(
            circle_center.x - galley.size().x / 2.,
            circle_center.y - circle_radius * 2.,
        );

        let label_shape = TextShape::new(label_pos, galley, color);
        res.push(label_shape.into());

        res
    }
    fn update(&mut self, state: &NodeProps<NV>) {
        self.pos = state.location();
        self.selected = state.selected;
        self.dragged = state.dragged;
        self.label_text = state.label.to_string();
        self.color = state.color();
        self.hidden = state.hidden;
        self.props.payload = state.payload.clone();
        self.props = state.clone();
    }
}

fn closest_point_on_circle(center: Pos2, radius: f32, dir: Vec2) -> Pos2 {
    center + dir.normalized() * radius
}

fn is_inside_circle(center: Pos2, radius: f32, pos: Pos2) -> bool {
    let dir = pos - center;
    dir.length() <= radius
}

#[cfg(test)]
mod test {
    use super::*;
    use egui::Pos2;

    #[test]
    fn test_closest_point_on_circle() {
        assert_eq!(
            closest_point_on_circle(Pos2::new(0.0, 0.0), 10.0, Vec2::new(5.0, 0.0)),
            Pos2::new(10.0, 0.0)
        );
        assert_eq!(
            closest_point_on_circle(Pos2::new(0.0, 0.0), 10.0, Vec2::new(15.0, 0.0)),
            Pos2::new(10.0, 0.0)
        );
        assert_eq!(
            closest_point_on_circle(Pos2::new(0.0, 0.0), 10.0, Vec2::new(0.0, 10.0)),
            Pos2::new(0.0, 10.0)
        );
    }

    #[test]
    fn test_is_inside_circle() {
        assert!(is_inside_circle(
            Pos2::new(0.0, 0.0),
            10.0,
            Pos2::new(5.0, 0.0)
        ));
        assert!(!is_inside_circle(
            Pos2::new(0.0, 0.0),
            10.0,
            Pos2::new(15.0, 0.0)
        ));
        assert!(is_inside_circle(
            Pos2::new(0.0, 0.0),
            10.0,
            Pos2::new(0.0, 10.0)
        ));
    }
}
