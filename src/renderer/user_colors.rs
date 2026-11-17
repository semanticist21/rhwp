//! 보기 > 컬러/회색조 > 사용자 색. 한컴 12.30은 편집 화면만 고대비 색으로 다시 그리고 문서·인쇄는 그대로 둔다.
//!
//! 한컴 12.30 Mac에서 확인한 역할(2026-11-18, 도움말 `view/user_color.htm`):
//! - 글자 색: 모든 글자(글자 색·하이퍼링크·수식 포함), 밑줄·취소선, 표 테두리, 도형 선, 도형 그림자.
//! - 배경 색: 종이, 셀 배경, 도형 면(단색·그러데이션·무늬), 글자 음영. 글자 그림자도 배경 색이라 보이지 않는다.
//! - 경고 색 1: 형광펜.
//! - 그림(개체·쪽 배경·채우기 그림)과 차트는 그대로 그린다.
//!
//! 색만 바꾸고 노드 배치는 그대로라 선택·적중은 바뀌지 않는다. 인쇄는 사용자 색을 끄고 그린다.

use super::render_tree::{
    BoundingBox, PageBackgroundNode, PageRenderTree, RenderNode, RenderNodeType,
};
use super::{GradientFillInfo, ShapeStyle, TextStyle};
use crate::model::color::{char_shade, opaque_rgb};
use crate::model::ColorRef;

/// 사용자 색 설정 창의 색(BGR). 경고 색 2·비활성 색은 앱이 그리는 격자·메모 칸에만 쓴다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserColors {
    pub text: ColorRef,
    pub background: ColorRef,
    pub highlight: ColorRef,
}

/// 페이지 트리의 색을 사용자 색으로 바꾼다.
pub(crate) fn apply_user_colors(tree: &mut PageRenderTree, colors: UserColors) {
    // 쪽 배경을 본문에만 칠한 쪽도 종이 전체를 배경 색으로 덮는다.
    let page = tree.root.bbox;
    let id = tree.frame.next_id();
    tree.root.children.insert(
        0,
        RenderNode::new(
            id,
            RenderNodeType::PageBackground(PageBackgroundNode {
                background_color: Some(colors.background),
                border_color: None,
                border_width: 0.0,
                gradient: None,
                image: None,
            }),
            BoundingBox::new(0.0, 0.0, page.width, page.height),
        ),
    );
    recolor(&mut tree.root, colors);
}

fn recolor(node: &mut RenderNode, colors: UserColors) {
    // 형광펜은 편집 화면에만 그리는 선 없는 칠 사각형이다(문단 배치의 RangeTag).
    let editor_only = node.editor_only;
    match &mut node.node_type {
        RenderNodeType::PageBackground(background) => {
            if background.background_color.is_some() || background.gradient.is_some() {
                background.background_color = Some(colors.background);
                background.gradient = None;
            }
            if background.border_color.is_some() {
                background.border_color = Some(colors.text);
            }
        }
        RenderNodeType::TextRun(run) => recolor_text(&mut run.style, colors),
        RenderNodeType::FootnoteMarker(marker) => {
            marker.color = colors.text;
            if let Some(run) = marker.source_run.as_deref_mut() {
                recolor_text(&mut run.style, colors);
            }
        }
        RenderNodeType::Line(line) => {
            line.style.color = colors.text;
            if line.underpaint_color.is_some() {
                line.underpaint_color = Some(colors.text);
            }
        }
        RenderNodeType::Rectangle(rect) => {
            let markpen = editor_only && rect.style.stroke_color.is_none();
            let fill = if markpen {
                colors.highlight
            } else {
                colors.background
            };
            recolor_shape(&mut rect.style, &mut rect.gradient, fill, colors);
        }
        RenderNodeType::Ellipse(ellipse) => recolor_shape(
            &mut ellipse.style,
            &mut ellipse.gradient,
            colors.background,
            colors,
        ),
        RenderNodeType::Path(path) => recolor_shape(
            &mut path.style,
            &mut path.gradient,
            colors.background,
            colors,
        ),
        RenderNodeType::Equation(equation) => {
            let css = css(colors.text);
            equation.svg_content = equation.svg_content.replace(&equation.color_str, &css);
            equation.color = colors.text;
            equation.color_str = css;
        }
        RenderNodeType::Placeholder(placeholder) => {
            placeholder.fill_color = colors.background;
            placeholder.stroke_color = colors.text;
        }
        RenderNodeType::FormObject(form) => {
            form.fore_color = css(colors.text);
            form.back_color = css(colors.background);
        }
        _ => {}
    }
    for child in &mut node.children {
        recolor(child, colors);
    }
}

fn recolor_text(style: &mut TextStyle, colors: UserColors) {
    style.color = colors.text;
    style.underline_color = colors.text;
    style.strike_color = colors.text;
    style.shadow_color = colors.background;
    if char_shade(style.shade_color).is_some() {
        style.shade_color = colors.background;
    }
    // 글자 채우기(그러데이션 등)도 글자 색 하나로 그린다.
    style.text_paint = None;
}

fn recolor_shape(
    style: &mut ShapeStyle,
    gradient: &mut Option<Box<GradientFillInfo>>,
    fill: ColorRef,
    colors: UserColors,
) {
    let filled = style.fill_color.and_then(opaque_rgb).is_some()
        || gradient.is_some()
        || style.pattern.is_some();
    if filled {
        style.fill_color = Some(fill);
    }
    *gradient = None;
    style.pattern = None;
    if style.stroke_color.and_then(opaque_rgb).is_some() {
        style.stroke_color = Some(colors.text);
    }
    if let Some(shadow) = &mut style.shadow {
        shadow.color = colors.text;
    }
}

/// BGR 색을 CSS `#rrggbb`로 쓴다.
pub(crate) fn css(color: ColorRef) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        color & 0xFF,
        (color >> 8) & 0xFF,
        (color >> 16) & 0xFF
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::render_tree::{ImageNode, LineNode, RectangleNode, TextRunNode};
    use crate::renderer::{LineStyle, ShadowStyle};

    const COLORS: UserColors = UserColors {
        text: 0x00FF_FFFF,
        background: 0x0000_0000,
        highlight: 0x0000_FF00,
    };

    fn node(tree: &mut PageRenderTree, kind: RenderNodeType) -> RenderNode {
        let id = tree.frame.next_id();
        RenderNode::new(id, kind, BoundingBox::new(10.0, 20.0, 30.0, 40.0))
    }

    fn run(style: TextStyle) -> RenderNodeType {
        RenderNodeType::TextRun(TextRunNode {
            text: "가".into(),
            style,
            char_shape_id: None,
            para_shape_id: None,
            section_index: None,
            para_index: None,
            char_start: None,
            cell_context: None,
            is_para_end: false,
            is_line_break_end: false,
            rotation: 0.0,
            is_vertical: false,
            char_overlap: None,
            border_fill_id: 0,
            baseline: 12.0,
            field_marker: Default::default(),
            layout_positions: None,
            display_text: None,
        })
    }

    fn rect(fill: Option<ColorRef>, stroke: Option<ColorRef>) -> RenderNodeType {
        RenderNodeType::Rectangle(RectangleNode::new(
            0.0,
            ShapeStyle {
                fill_color: fill,
                stroke_color: stroke,
                ..Default::default()
            },
            None,
        ))
    }

    /// 한컴 12.30 사용자 색: 글자·선은 글자 색, 면·음영은 배경 색, 형광펜은 경고 색 1, 그림은 그대로다.
    #[test]
    fn recolors_roles_like_hancom_user_color_view() {
        let mut tree = PageRenderTree::new(0, 100.0, 200.0);
        let body_background = node(
            &mut tree,
            RenderNodeType::PageBackground(PageBackgroundNode {
                background_color: Some(0x00FF_FFFF),
                border_color: None,
                border_width: 0.0,
                gradient: None,
                image: None,
            }),
        );
        let red = node(
            &mut tree,
            run(TextStyle {
                color: 0x0000_00FF,
                underline_color: 0x0000_00FF,
                shade_color: 0x00FF_C000,
                shadow_color: 0x00FF_00FF,
                ..Default::default()
            }),
        );
        let markpen = node(&mut tree, rect(Some(0x0000_FFFF), None)).with_editor_only();
        let mut shape = node(&mut tree, rect(Some(0x0000_FFFF), Some(0x00FF_0000)));
        if let RenderNodeType::Rectangle(rect) = &mut shape.node_type {
            rect.style.shadow = Some(ShadowStyle {
                shadow_type: 1,
                color: 0x0000_00FF,
                offset_x: 2.0,
                offset_y: 2.0,
                alpha: 0,
            });
        }
        let empty = node(&mut tree, rect(None, None));
        let border = node(
            &mut tree,
            RenderNodeType::Line(LineNode::new(
                0.0,
                0.0,
                10.0,
                0.0,
                LineStyle {
                    color: 0x0000_00FF,
                    ..Default::default()
                },
            )),
        );
        let picture = node(
            &mut tree,
            RenderNodeType::Image(ImageNode::new(1, Some(vec![1, 2, 3]))),
        );
        tree.root.children = vec![body_background, red, markpen, shape, empty, border, picture];

        apply_user_colors(&mut tree, COLORS);

        let children = &tree.root.children;
        // 맨 앞에 종이 전체를 덮는 배경 색을 깐다.
        let RenderNodeType::PageBackground(paper) = &children[0].node_type else {
            unreachable!()
        };
        assert_eq!(paper.background_color, Some(0));
        assert_eq!(
            (children[0].bbox.width, children[0].bbox.height),
            (100.0, 200.0)
        );
        let RenderNodeType::PageBackground(body) = &children[1].node_type else {
            unreachable!()
        };
        assert_eq!(body.background_color, Some(0));
        let RenderNodeType::TextRun(red) = &children[2].node_type else {
            unreachable!()
        };
        assert_eq!(
            (
                red.style.color,
                red.style.underline_color,
                red.style.shade_color,
                red.style.shadow_color
            ),
            (0x00FF_FFFF, 0x00FF_FFFF, 0, 0)
        );
        let fill = |index: usize| match &children[index].node_type {
            RenderNodeType::Rectangle(rect) => (
                rect.style.fill_color,
                rect.style.stroke_color,
                rect.style.shadow.as_ref().map(|shadow| shadow.color),
            ),
            _ => unreachable!(),
        };
        assert_eq!(fill(3), (Some(0x0000_FF00), None, None));
        assert_eq!(fill(4), (Some(0), Some(0x00FF_FFFF), Some(0x00FF_FFFF)));
        // 칠하지 않은 도형은 투명하게 둔다.
        assert_eq!(fill(5), (None, None, None));
        let RenderNodeType::Line(line) = &children[6].node_type else {
            unreachable!()
        };
        assert_eq!(line.style.color, 0x00FF_FFFF);
        let RenderNodeType::Image(image) = &children[7].node_type else {
            unreachable!()
        };
        assert_eq!(image.data.as_deref(), Some(&[1, 2, 3][..]));
    }

    /// 사용자 색은 화면 트리만 바꾸고 문서·저장은 그대로 두며, 끄면 본래 색으로 그린다.
    #[test]
    fn user_colors_change_only_the_page_tree() {
        let mut doc = crate::wasm_api::HwpDocument::create_empty();
        doc.insert_text_native(0, 0, 0, "글자").unwrap();
        let saved = doc.export_hwp_native().unwrap();
        let text_color = |doc: &crate::wasm_api::HwpDocument| {
            fn find(node: &RenderNode) -> Option<ColorRef> {
                match &node.node_type {
                    RenderNodeType::TextRun(run) if !run.text.is_empty() => Some(run.style.color),
                    _ => node.children.iter().find_map(find),
                }
            }
            find(&doc.build_page_tree(0).unwrap().root)
        };
        assert_eq!(text_color(&doc), Some(0));

        doc.user_colors = Some(COLORS);
        doc.invalidate_page_tree_cache();
        assert_eq!(text_color(&doc), Some(0x00FF_FFFF));
        assert_eq!(doc.export_hwp_native().unwrap(), saved);

        doc.user_colors = None;
        doc.invalidate_page_tree_cache();
        assert_eq!(text_color(&doc), Some(0));
    }

    #[test]
    fn writes_css_from_bgr() {
        assert_eq!(css(0x0000_00FF), "#ff0000");
        assert_eq!(css(0x00FF_0000), "#0000ff");
    }
}
