//! 각주·미주 문단의 떠 있는(글자처럼 취급하지 않은) 그림·도형을 주석 영역에 배치한다.
//!
//! 한컴 12.30(Mac) 실측: 주석 안 떠 있는 개체의 가로 기준은 문단이면 문단 왼쪽, 단·쪽·종이면 주석
//! 영역 왼쪽이다. 세로 기준은 문단·쪽이면 주석 문단 위에서 잰다. 종이 기준 개체는 그리지 않는다.
//! 문단 기준 개체는 그 아래 끝까지 주석 높이에 넣고(쪽 기준은 넣지 않는다), 자리차지 개체가 첫 줄에
//! 걸치면 글을 개체 아래에서 시작한다.
use super::*;
use crate::model::shape::{CommonObjAttr, TextWrap, VertAlign, VertRelTo};

/// 주석 문단의 떠 있는 그림·도형.
fn note_floats(para: &Paragraph) -> impl Iterator<Item = (usize, &CommonObjAttr)> {
    para.controls
        .iter()
        .enumerate()
        .filter_map(|(index, control)| match control {
            Control::Picture(picture) if !picture.common.treat_as_char => {
                Some((index, &picture.common))
            }
            Control::Shape(shape) if !shape.common().treat_as_char => Some((index, shape.common())),
            _ => None,
        })
}

/// 문단 기준·위 정렬 개체의 문단 위에서 아래 끝까지(HWPUNIT, 바깥 여백 포함).
fn para_top_aligned_bottom_hu(common: &CommonObjAttr) -> Option<i32> {
    (common.vert_rel_to == VertRelTo::Para
        && matches!(common.vert_align, VertAlign::Top | VertAlign::Inside))
    .then(|| {
        crate::renderer::float_placement::signed_hwpunit(common.vertical_offset)
            + common.height as i32
            + i32::from(common.margin.top)
            + i32::from(common.margin.bottom)
    })
}

/// 한컴처럼 주석 높이에 넣는 문단 기준 떠 있는 개체의 아래 끝(문단 위에서, px). 없으면 0이다.
pub(crate) fn note_float_extent_px(para: &Paragraph, dpi: f64) -> f64 {
    note_floats(para)
        .filter_map(|(_, common)| para_top_aligned_bottom_hu(common))
        .map(|bottom| hwpunit_to_px(bottom, dpi))
        .fold(0.0, f64::max)
}

/// 문단 기준 자리차지 개체가 있는 주석 문단이면, 첫 줄에 걸친 개체 아래 끝(문단 위에서, px)을 준다.
/// 걸친 개체가 없으면 0, 자리차지 개체가 없으면 `None`이다. 한컴 저장 줄도 첫 줄을 이 자리에 둔다.
pub(crate) fn note_topbottom_lead_px(para: &Paragraph, dpi: f64) -> Option<f64> {
    let first_line = para.line_segs.first().map_or(0, |line| line.line_height);
    let mut found = false;
    let lead = note_floats(para)
        .filter(|(_, common)| {
            common.text_wrap == TextWrap::TopAndBottom && common.vert_rel_to == VertRelTo::Para
        })
        .filter_map(|(_, common)| {
            found = true;
            let top = crate::renderer::float_placement::signed_hwpunit(common.vertical_offset);
            para_top_aligned_bottom_hu(common).filter(|_| top < first_line.max(1))
        })
        .map(|bottom| hwpunit_to_px(bottom, dpi))
        .fold(0.0, f64::max);
    found.then_some(lead)
}

/// 자리차지 문단의 저장 줄 위치를 첫 줄 기준으로 옮긴 사본. 줄 배치는 그 문단의 줄을 단 위가 아니라
/// 주어진 영역 위에서 저장 위치로 놓으므로, 주석 글이 개체 아래에서 저장 줄 간격대로 흐르게 한다.
pub(crate) fn rebased_note_paragraph(para: &Paragraph, first_line: usize) -> Paragraph {
    let mut owned = para.clone();
    if let Some(base) = owned
        .line_segs
        .get(first_line)
        .map(|line| line.vertical_pos)
    {
        for line in &mut owned.line_segs {
            line.vertical_pos -= base;
        }
    }
    owned
}

/// 개체 배치가 문단 번호(`from`)로 단 표지를 각주 표지(`to`)로 바꾼다. 각주 주소 붙이기가 그 표지를 읽는다.
fn relabel_para(node: &mut RenderNode, from: usize, to: usize) {
    let fix = |para: &mut Option<usize>| {
        if *para == Some(from) {
            *para = Some(to);
        }
    };
    let fix_context = |context: Option<&mut CellContext>| {
        if let Some(context) = context.filter(|context| context.parent_para_index == from) {
            context.parent_para_index = to;
        }
    };
    match &mut node.node_type {
        RenderNodeType::Rectangle(n) => fix(&mut n.para_index),
        RenderNodeType::Ellipse(n) => fix(&mut n.para_index),
        RenderNodeType::Line(n) => fix(&mut n.para_index),
        RenderNodeType::Path(n) => fix(&mut n.para_index),
        RenderNodeType::Group(n) => fix(&mut n.para_index),
        RenderNodeType::Equation(n) => fix(&mut n.para_index),
        RenderNodeType::Image(n) => {
            fix(&mut n.para_index);
            fix_context(n.cell_context.as_mut());
        }
        RenderNodeType::TextRun(n) => fix_context(n.cell_context.as_mut()),
        // 주석 안 차트 같은 OLE도 각주 표지를 달아야 쪽 개체 목록이 본문 문단 주소로 바꾼다.
        RenderNodeType::RawSvg(crate::renderer::render_tree::RawSvgNode {
            control_ref,
            cell_context,
            ..
        })
        | RenderNodeType::Placeholder(crate::renderer::render_tree::PlaceholderNode {
            control_ref,
            cell_context,
            ..
        }) => {
            if let Some(control_ref) = control_ref.as_mut().filter(|r| r.para_index == from) {
                control_ref.para_index = to;
            }
            fix_context(cell_context.as_mut());
        }
        RenderNodeType::Table(n) => {
            fix(&mut n.para_index);
            fix_context(n.cell_context.as_mut());
        }
        _ => {}
    }
    for child in &mut node.children {
        relabel_para(child, from, to);
    }
}

impl LayoutEngine {
    /// 주석 문단의 떠 있는 그림·도형을 그린다. 글 뒤 개체는 `behind`, 나머지는 `front`에 담아 부르는
    /// 쪽이 글줄 앞뒤에 둔다. `markers`는 각주 표지(각주 순번, 각주 문단 표지)다.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn layout_note_floats(
        &self,
        tree: &mut PageLayoutContext,
        paragraphs: &[Paragraph],
        para_index: usize,
        (marker_section, marker_para): (usize, usize),
        styles: &ResolvedStyleSet,
        area: &LayoutRect,
        para_top: f64,
        bin_data_content: &[BinDataContent],
        behind: &mut Vec<RenderNode>,
        front: &mut Vec<RenderNode>,
    ) {
        let Some(para) = paragraphs.get(para_index) else {
            return;
        };
        let style = styles.para_styles.get(para.para_shape_id as usize);
        let margin_left = style.map_or(0.0, |style| style.margin_left);
        let margin_right = style.map_or(0.0, |style| style.margin_right);
        let alignment = style.map_or(Alignment::Left, |style| style.alignment);
        // 쪽 기준 세로 위치도 주석 문단 위에서 잰다(한컴 실측). 가로 단·쪽·종이는 주석 영역 왼쪽이다.
        let body = LayoutRect {
            y: para_top,
            ..*area
        };
        let container = LayoutRect {
            x: area.x + margin_left,
            y: para_top,
            width: (area.width - margin_left - margin_right).max(0.0),
            height: area.height,
        };
        for (control_index, common) in note_floats(para) {
            if common.vert_rel_to == VertRelTo::Paper {
                continue;
            }
            let mut holder = RenderNode::new(
                tree.next_id(),
                RenderNodeType::FootnoteArea,
                layout_rect_to_bbox(area),
            );
            match &para.controls[control_index] {
                Control::Picture(picture) => {
                    self.layout_body_picture(
                        tree,
                        &mut holder,
                        picture,
                        &container,
                        area,
                        &body,
                        area,
                        bin_data_content,
                        styles,
                        alignment,
                        para_top,
                        marker_section,
                        marker_para,
                        control_index,
                        None,
                    );
                }
                Control::Shape(_) => {
                    self.layout_shape(
                        tree,
                        &mut holder,
                        paragraphs,
                        para_index,
                        control_index,
                        marker_section,
                        styles,
                        area,
                        &body,
                        area,
                        para_top,
                        alignment,
                        bin_data_content,
                        &std::collections::HashMap::new(),
                        false,
                        None,
                    );
                    relabel_para(&mut holder, para_index, marker_para);
                }
                _ => continue,
            }
            let target = if common.text_wrap == TextWrap::BehindText {
                &mut *behind
            } else {
                &mut *front
            };
            target.append(&mut holder.children);
        }
    }
}
