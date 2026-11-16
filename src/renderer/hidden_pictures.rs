//! 보기 > 그림을 끈 편집 화면. 한컴 12.30은 그림 파일을 그리지 않고 그림이 들어간 자리만 표시한다.
//!
//! 한컴 12.30 Mac에서 확인한 대상(2026-11-16):
//! - 그림 개체(본문·표 셀·머리말·바탕쪽·묶음 안): 회색 파선 상자와 두 대각선만 그린다.
//! - 쪽 배경·셀 배경(테두리/배경) 그림: 그리지 않는다.
//! - 도형 면을 채운 그림: 그대로 그린다(한컴 도움말 `view/displaying_figures.htm`).
//!
//! 그림 노드는 숨긴 채 트리에 남겨 개체 배치(선택·적중)는 그대로다. 인쇄는 그림을 켜고 그린다.

use super::render_tree::{
    BoundingBox, LineNode, NodeId, PageRenderTree, RenderNode, RenderNodeType,
};
use super::{LineStyle, StrokeDash};

/// 자리 표시 선 색(BGR). 한컴 화면의 회색 파선이다.
const PLACEHOLDER_COLOR: u32 = 0x0080_8080;
const PLACEHOLDER_WIDTH: f64 = 0.5;
/// 한컴 화면의 짧은 파선(px 단위 켬/끔).
const PLACEHOLDER_DASH: [f64; 2] = [3.0, 2.0];

/// 페이지 트리의 그림을 한컴 그림 숨기기 화면처럼 바꾼다.
pub(crate) fn hide_pictures(tree: &mut PageRenderTree) {
    let mut next_id = || tree.frame.next_id();
    hide_in(&mut tree.root, &mut next_id);
}

fn hide_in(node: &mut RenderNode, next_id: &mut dyn FnMut() -> NodeId) {
    if let RenderNodeType::PageBackground(background) = &mut node.node_type {
        background.image = None;
    }
    let owner = matches!(
        node.node_type,
        RenderNodeType::Rectangle(_) | RenderNodeType::Ellipse(_) | RenderNodeType::Path(_)
    );
    let mut index = 0;
    while index < node.children.len() {
        let child = &mut node.children[index];
        let placeholder = match &child.node_type {
            RenderNodeType::Image(image) if image.fill_mode.is_some() => {
                // 도형의 그림 채우기는 도형 노드의 자식이다. 그 밖의 채우기는 셀·문단 배경이다.
                if !owner {
                    child.visible = false;
                }
                false
            }
            // 문단 머리의 그림 글머리표는 그림 개체가 아니다(문단 번호만 있고 개체 번호가 없다).
            // ponytail: 한컴에서 확인하지 않아 그대로 그린다. 확인하면 이 갈래를 고친다.
            RenderNodeType::Image(image)
                if image.para_index.is_some() && image.control_index.is_none() =>
            {
                false
            }
            RenderNodeType::Image(_) => child.visible,
            _ => {
                hide_in(child, next_id);
                false
            }
        };
        if placeholder {
            child.visible = false;
            let bbox = child.bbox;
            let lines = placeholder_lines(bbox, next_id);
            let count = lines.len();
            node.children.splice(index + 1..index + 1, lines);
            index += count;
        }
        index += 1;
    }
}

/// 그림 자리: 상자 네 변과 두 대각선. 돌린 그림도 돌리기 전 상자로 그린다.
/// ponytail: 회전·대칭 그림의 자리 상자는 축에 맞춘 bbox다. 한컴과 다르면 transform을 따라 돌린다.
fn placeholder_lines(bbox: BoundingBox, next_id: &mut dyn FnMut() -> NodeId) -> Vec<RenderNode> {
    let (left, top) = (bbox.x, bbox.y);
    let (right, bottom) = (bbox.x + bbox.width, bbox.y + bbox.height);
    [
        (left, top, right, top),
        (right, top, right, bottom),
        (right, bottom, left, bottom),
        (left, bottom, left, top),
        (left, top, right, bottom),
        (right, top, left, bottom),
    ]
    .into_iter()
    .map(|(x1, y1, x2, y2)| {
        let line = LineNode::new(
            x1,
            y1,
            x2,
            y2,
            LineStyle {
                color: PLACEHOLDER_COLOR,
                width: PLACEHOLDER_WIDTH,
                dash: StrokeDash::Dash,
                dash_pattern: Some(PLACEHOLDER_DASH.to_vec()),
                ..Default::default()
            },
        );
        let bbox = line.ink_bbox();
        RenderNode::new(next_id(), RenderNodeType::Line(line), bbox)
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::image::ImageEffect;
    use crate::model::style::ImageFillMode;
    use crate::renderer::render_tree::{
        ImageNode, PageBackgroundImage, PageBackgroundNode, RectangleNode,
    };
    use crate::renderer::ShapeStyle;

    fn node(tree: &mut PageRenderTree, kind: RenderNodeType) -> RenderNode {
        let id = tree.frame.next_id();
        RenderNode::new(id, kind, BoundingBox::new(10.0, 20.0, 30.0, 40.0))
    }

    fn image(owner: Option<(usize, usize)>, fill: bool) -> RenderNodeType {
        let mut image = ImageNode::new(1, Some(vec![1]));
        if let Some((para, control)) = owner {
            image.para_index = Some(para);
            image.control_index = Some(control);
        }
        if fill {
            image.fill_mode = Some(ImageFillMode::TileAll);
        }
        RenderNodeType::Image(image)
    }

    /// 한컴 12.30 그림 숨기기: 그림 개체는 자리 상자로, 쪽·셀 배경 그림은 빼고, 도형 채우기·글머리표는 그대로다.
    #[test]
    fn hides_pictures_like_hancom_picture_view_off() {
        let mut tree = PageRenderTree::new(0, 100.0, 100.0);
        let background = node(
            &mut tree,
            RenderNodeType::PageBackground(PageBackgroundNode {
                background_color: None,
                border_color: None,
                border_width: 0.0,
                gradient: None,
                image: Some(PageBackgroundImage {
                    data: vec![1],
                    fill_mode: ImageFillMode::TileAll,
                    brightness: 0,
                    contrast: 0,
                    effect: ImageEffect::RealPic,
                }),
            }),
        );
        let picture = node(&mut tree, image(Some((0, 1)), false));
        // 묶음 안 그림은 개체 번호가 없다.
        let grouped = node(&mut tree, image(None, false));
        let mut bullet = node(&mut tree, image(Some((0, 0)), false));
        if let RenderNodeType::Image(image) = &mut bullet.node_type {
            image.control_index = None;
        }
        let cell_fill = node(&mut tree, image(None, true));
        let mut shape = node(
            &mut tree,
            RenderNodeType::Rectangle(RectangleNode::new(0.0, ShapeStyle::default(), None)),
        );
        shape.children.push(node(&mut tree, image(None, true)));
        let mut column = node(&mut tree, RenderNodeType::Column(0));
        column.children = vec![picture, grouped, bullet, cell_fill, shape];
        tree.root.children = vec![background, column];

        hide_pictures(&mut tree);

        let RenderNodeType::PageBackground(background) = &tree.root.children[0].node_type else {
            unreachable!()
        };
        assert!(background.image.is_none());
        let children = &tree.root.children[1].children;
        let summary: Vec<_> = children
            .iter()
            .map(|child| {
                let kind = match child.node_type {
                    RenderNodeType::Image(_) => "image",
                    RenderNodeType::Line(_) => "line",
                    RenderNodeType::Rectangle(_) => "shape",
                    _ => "other",
                };
                (kind, child.visible)
            })
            .collect();
        let placeholder = [("line", true); 6];
        let mut expected = vec![("image", false)];
        expected.extend(placeholder);
        expected.push(("image", false));
        expected.extend(placeholder);
        expected.extend([("image", true), ("image", false), ("shape", true)]);
        assert_eq!(summary, expected);
        // 도형 면 채우기 그림은 그대로 그린다.
        assert!(children.last().unwrap().children[0].visible);
        // 자리 상자는 그림 상자의 네 변과 두 대각선이다.
        let RenderNodeType::Line(diagonal) = &children[5].node_type else {
            unreachable!()
        };
        assert_eq!(
            (diagonal.x1, diagonal.y1, diagonal.x2, diagonal.y2),
            (10.0, 20.0, 40.0, 60.0)
        );
    }
}
