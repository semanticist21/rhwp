//! 그림으로 채운 도형의 평행 그림자는 한컴 12.30처럼 그림자 색 면으로 칠한다(테두리만 긋지 않는다).
#![cfg(not(target_arch = "wasm32"))]

use rhwp::paint::{LayerBuilder, LayerNode, LayerNodeKind, PaintOp, RenderProfile};
use rhwp::renderer::render_tree::{
    BoundingBox, ImageNode, PageRenderTree, RectangleNode, RenderNode, RenderNodeType,
};
use rhwp::renderer::{ShadowStyle, ShapeStyle};

fn rectangles(node: &LayerNode, out: &mut Vec<RectangleNode>) {
    match &node.kind {
        LayerNodeKind::Group { children, .. } => children.iter().for_each(|c| rectangles(c, out)),
        LayerNodeKind::ClipRect { child, .. } | LayerNodeKind::ClipPath { child, .. } => {
            rectangles(child, out)
        }
        LayerNodeKind::Leaf { ops } => out.extend(ops.iter().filter_map(|op| match op {
            PaintOp::Rectangle { rect, .. } => Some(rect.as_ref().clone()),
            _ => None,
        })),
    }
}

#[test]
fn picture_filled_rectangle_shadow_is_a_filled_silhouette() {
    let style = ShapeStyle {
        stroke_color: Some(0),
        stroke_width: 1.0,
        shadow: Some(ShadowStyle {
            shadow_type: 4,
            color: 0x808080,
            offset_x: 0.0,
            offset_y: 0.0,
            alpha: 0,
        }),
        ..ShapeStyle::default()
    };
    let bbox = BoundingBox::new(50.0, 50.0, 200.0, 100.0);
    let mut rect = RenderNode::new(
        2,
        RenderNodeType::Rectangle(RectangleNode::new(0.0, style, None)),
        bbox,
    );
    let fill = ImageNode {
        fill_mode: Some(rhwp::model::style::ImageFillMode::FitToSize),
        ..ImageNode::new(1, Some(vec![1, 2, 3]))
    };
    rect.children
        .push(RenderNode::new(3, RenderNodeType::Image(fill), bbox));
    let mut tree = PageRenderTree::new(0, 800.0, 600.0);
    tree.root.children.push(rect);
    let layers = LayerBuilder::new(RenderProfile::Screen).build(&tree);
    let mut found = Vec::new();
    rectangles(&layers.root, &mut found);
    assert!(
        found.iter().any(|r| r.style.fill_color == Some(0x808080)),
        "그림자 면: {:?}",
        found.iter().map(|r| r.style.fill_color).collect::<Vec<_>>()
    );
}
