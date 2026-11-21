//! 수용 단계에서 확정한 소유 단의 각주를 같은 폭과 줄 메트릭으로 배치한다.
use super::*;
use crate::renderer::footnote_frames::FramePolicy;
use std::collections::BTreeMap;

/// 글줄의 잉크가 아니라 실제 수용한 본문 흐름의 끝을 단별로 공유한다.
pub(super) struct BodyFlowBottoms {
    start: f64,
    maximum: f64,
    columns: BTreeMap<u16, f64>,
}

impl BodyFlowBottoms {
    pub(super) fn new(start: f64) -> Self {
        Self {
            start,
            maximum: start,
            columns: BTreeMap::new(),
        }
    }

    pub(super) fn record(&mut self, column: u16, bottom: f64) {
        if bottom.is_finite() {
            self.maximum = self.maximum.max(bottom);
            self.columns
                .entry(column)
                .and_modify(|end| *end = end.max(bottom))
                .or_insert(bottom);
        }
    }

    fn for_column(&self, column: u16) -> f64 {
        self.columns.get(&column).copied().unwrap_or(self.start)
    }
}

impl LayoutEngine {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn build_footnote_area(
        &self,
        tree: &mut PageRenderTree,
        page: &PageContent,
        paragraphs: &[Paragraph],
        shape: &FootnoteShape,
        styles: &ResolvedStyleSet,
        layout: &PageLayoutInfo,
        images: &[BinDataContent],
        body_bottoms: &BodyFlowBottoms,
    ) {
        let policy = FramePolicy {
            placement: shape.placement,
            columns: layout.column_areas.len() as u16,
        };
        let mut groups = BTreeMap::<u16, Vec<usize>>::new();
        for (index, note) in page.footnotes.iter().enumerate() {
            groups
                .entry(policy.target(note.column_index))
                .or_default()
                .push(index);
        }
        for (column, indices) in groups {
            let notes = indices
                .iter()
                .map(|&index| page.footnotes[index].clone())
                .collect::<Vec<_>>();
            let mut area = policy.area(layout, column);
            area.height =
                self.estimate_footnote_area_height(&notes, paragraphs, shape, styles, area.width);
            area.y = layout.body_area.y + layout.body_area.height - area.height;
            if shape.print_inline_after_text {
                area.y = if policy.spans_page() {
                    body_bottoms.maximum
                } else {
                    body_bottoms.for_column(column)
                };
            }

            let id = tree.next_id();
            let mut node =
                RenderNode::new(id, RenderNodeType::FootnoteArea, layout_rect_to_bbox(&area));
            self.layout_footnote_area(
                tree.frame_mut(),
                &mut node,
                &notes,
                paragraphs,
                styles,
                &area,
                shape,
                &indices,
                images,
            );
            address_footnote_images(&mut node, &page.footnotes, page.section_index);
            tree.root.children.push(node);
        }
    }
}

/// 각주 글줄 배치는 그림에 (각주 순번, `usize::MAX - 2000 - 각주 문단`) 표지를 단다. 본문 문단 각주
/// 바로 안 그림은 쪽 개체 목록·셀 경로 API가 고를 수 있게 [(각주, 0, 각주 문단)] 문서 주소로 바꾼다.
fn address_footnote_images(node: &mut RenderNode, notes: &[FootnoteRef], section: usize) {
    if let RenderNodeType::Image(ref mut img) = node.node_type {
        if let (Some(index), Some(marker), None) =
            (img.section_index, img.para_index, img.cell_context.as_ref())
        {
            let source = (usize::MAX - 3000..usize::MAX - 1000)
                .contains(&marker)
                .then(|| notes.get(index))
                .flatten()
                .map(|note| &note.source);
            if let Some(crate::renderer::pagination::FootnoteSource::Body {
                para_index,
                control_index,
            }) = source
            {
                img.section_index = Some(section);
                img.para_index = Some(*para_index);
                img.cell_context = Some(CellContext {
                    in_textbox: false,
                    parent_para_index: *para_index,
                    path: vec![CellPathEntry {
                        control_index: *control_index,
                        cell_index: 0,
                        cell_para_index: usize::MAX - 2000 - marker,
                        text_direction: 0,
                    }],
                });
            }
        }
    }
    for child in &mut node.children {
        address_footnote_images(child, notes, section);
    }
}

/// 미주 글줄 배치는 미주 문단을 본문 문단 뒤 번호(`base + 차례`)로 그린다. 본문 문단 미주 바로 안
/// 그림과 그 캡션 글줄은 쪽 개체 목록·셀 경로 API가 고를 수 있게 [(미주, 0, 미주 문단)] 문서 주소로
/// 바꾼다. 표 셀 안 미주는 그 경로를 아직 나타내지 않는다.
pub(crate) fn address_endnote_controls(
    node: &mut RenderNode,
    base: usize,
    sources: &[EndnoteParaSource],
) {
    fn walk(node: &mut RenderNode, base: usize, sources: &[EndnoteParaSource], caption: bool) {
        let source = |para: usize| {
            para.checked_sub(base)
                .and_then(|index| sources.get(index))
                .filter(|source| source.cell_path.is_empty())
        };
        let note_step = |source: &EndnoteParaSource| CellPathEntry {
            control_index: source.control_index,
            cell_index: 0,
            cell_para_index: source.note_para_index,
            text_direction: 0,
        };
        let mut caption = caption;
        match node.node_type {
            RenderNodeType::Image(ref mut img) if img.cell_context.is_none() => {
                if let Some(source) = img.para_index.and_then(source) {
                    img.section_index = Some(source.section_index);
                    img.para_index = Some(source.para_index);
                    img.cell_context = Some(CellContext {
                        in_textbox: false,
                        parent_para_index: source.para_index,
                        path: vec![note_step(source)],
                    });
                }
            }
            RenderNodeType::TextLine(ref line) => caption = line.caption_owner.is_some(),
            RenderNodeType::TextRun(ref mut run) if caption => {
                if let Some(ctx) = run.cell_context.as_mut() {
                    if let Some(source) = source(ctx.parent_para_index) {
                        ctx.path.insert(0, note_step(source));
                        ctx.parent_para_index = source.para_index;
                    }
                }
            }
            _ => {}
        }
        for child in &mut node.children {
            walk(child, base, sources, caption);
        }
    }
    walk(node, base, sources, false);
}
