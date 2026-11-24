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
            address_footnote_controls(&mut node, &page.footnotes, page.section_index);
            tree.root.children.push(node);
        }
    }
}

/// 각주 출처와 각주 문단 → (본문 문단, [..각주까지의 셀 경로, (각주, 0, 각주 문단)]).
/// 본문·표 칸·글상자 안 각주 모두 본문 문단을 뿌리로 한 `...ByPath` API 주소다.
pub(super) fn footnote_note_path(
    source: &crate::renderer::pagination::FootnoteSource,
    note_para: usize,
) -> (usize, Vec<CellPathEntry>) {
    let (host, steps, note_control) = source.note_owner();
    let entry = |control_index, cell_index, cell_para_index| CellPathEntry {
        control_index,
        cell_index,
        cell_para_index,
        text_direction: 0,
    };
    let mut path: Vec<CellPathEntry> = steps
        .into_iter()
        .map(|(control, cell, para)| entry(control, cell, para))
        .collect();
    path.push(entry(note_control, 0, note_para));
    (host, path)
}

/// 각주 표지(각주 순번, `usize::MAX - 2000 - 각주 문단`)이면 문서 구역·본문 문단으로 바꾸고 각주
/// 경로를 준다.
fn footnote_owner(
    notes: &[FootnoteRef],
    section: usize,
    section_slot: &mut Option<usize>,
    para_marker: usize,
) -> Option<(usize, Vec<CellPathEntry>)> {
    if !(usize::MAX - 3000..usize::MAX - 1000).contains(&para_marker) {
        return None;
    }
    let note = notes.get((*section_slot)?)?;
    *section_slot = Some(section);
    Some(footnote_note_path(
        &note.source,
        usize::MAX - 2000 - para_marker,
    ))
}

/// 각주 표지를 단 셀 문맥(각주 안 표 칸·글상자 글, 그 안 그림)을 본문 문단 뿌리로 잇는다.
fn readdress_footnote_context(
    notes: &[FootnoteRef],
    section: usize,
    section_slot: &mut Option<usize>,
    ctx: &mut CellContext,
) -> Option<usize> {
    let (host, mut path) = footnote_owner(notes, section, section_slot, ctx.parent_para_index)?;
    path.append(&mut ctx.path);
    ctx.parent_para_index = host;
    ctx.path = path;
    Some(host)
}

/// 각주 글줄 배치는 개체에 (각주 순번, `usize::MAX - 2000 - 각주 문단`) 표지를 단다. 각주 바로 안
/// 그림·수식·도형·표와 각주 안 표 칸·글상자 글은 쪽 개체 목록·셀 경로 API가 고를 수 있게
/// 본문 문단을 뿌리로 한 [..각주까지의 경로, (각주, 0, 각주 문단)] 문서 주소로 바꾼다.
/// 각주 글줄 자체(셀 문맥 없는 글)는 각주 히트 테스트가 쓰는 표지를 그대로 둔다.
fn address_footnote_controls(node: &mut RenderNode, notes: &[FootnoteRef], section: usize) {
    let RenderNode {
        node_type,
        container_path,
        ..
    } = &mut *node;
    // 도형 노드: 주석 경로는 쪽 개체 목록이 `cellPath` 로 내는 컨테이너 경로가 된다.
    let mut shape = |sec: &mut Option<usize>, para: &mut Option<usize>| {
        if let Some((host, mut path)) =
            para.and_then(|marker| footnote_owner(notes, section, sec, marker))
        {
            *para = Some(host);
            path.append(container_path);
            *container_path = path;
        }
    };
    match node_type {
        RenderNodeType::Image(img) => match img.cell_context.as_mut() {
            Some(ctx) => {
                if let Some(host) =
                    readdress_footnote_context(notes, section, &mut img.section_index, ctx)
                {
                    img.para_index = Some(host);
                }
            }
            None => {
                if let Some((host, path)) = img.para_index.and_then(|marker| {
                    footnote_owner(notes, section, &mut img.section_index, marker)
                }) {
                    img.para_index = Some(host);
                    img.cell_context = Some(CellContext {
                        in_textbox: false,
                        parent_para_index: host,
                        path,
                    });
                }
            }
        },
        RenderNodeType::TextRun(run) => {
            if let Some(ctx) = run.cell_context.as_mut() {
                readdress_footnote_context(notes, section, &mut run.section_index, ctx);
            }
        }
        RenderNodeType::Table(table) => match table.cell_context.as_mut() {
            Some(ctx) => {
                if let Some(host) =
                    readdress_footnote_context(notes, section, &mut table.section_index, ctx)
                {
                    table.para_index = Some(host);
                }
            }
            None => {
                // 표 노드의 경로는 마지막 마디가 표 자신이다(`nested_table_meta`).
                if let (Some((host, mut path)), Some(control_index)) = (
                    table.para_index.and_then(|marker| {
                        footnote_owner(notes, section, &mut table.section_index, marker)
                    }),
                    table.control_index,
                ) {
                    path.push(CellPathEntry {
                        control_index,
                        cell_index: 0,
                        cell_para_index: 0,
                        text_direction: 0,
                    });
                    table.para_index = Some(host);
                    table.cell_context = Some(CellContext {
                        in_textbox: false,
                        parent_para_index: host,
                        path,
                    });
                }
            }
        },
        RenderNodeType::Equation(eq) if eq.cell_index.is_none() => {
            shape(&mut eq.section_index, &mut eq.para_index)
        }
        RenderNodeType::Rectangle(n) => shape(&mut n.section_index, &mut n.para_index),
        RenderNodeType::Ellipse(n) => shape(&mut n.section_index, &mut n.para_index),
        RenderNodeType::Line(n) => shape(&mut n.section_index, &mut n.para_index),
        RenderNodeType::Path(n) => shape(&mut n.section_index, &mut n.para_index),
        RenderNodeType::Group(n) => shape(&mut n.section_index, &mut n.para_index),
        _ => {}
    }
    for child in &mut node.children {
        address_footnote_controls(child, notes, section);
    }
}

/// 미주 글줄 배치는 미주 문단을 본문 문단 뒤 번호(`base + 차례`)로 그린다. 미주(본문·표 칸·글상자
/// 안 미주) 바로 안 그림·수식·도형·표와 미주 안 표 칸·글상자·캡션 글줄은 쪽 개체 목록·셀 경로
/// API가 고를 수 있게 본문 문단을 뿌리로 한 [..미주까지의 경로, (미주, 0, 미주 문단)] 문서
/// 주소로 바꾼다. 미주 글줄 자체(셀 문맥 없는 글)는 미주 히트 테스트가 쓰는 번호를 그대로 둔다.
pub(crate) fn address_endnote_controls(
    node: &mut RenderNode,
    base: usize,
    sources: &[EndnoteParaSource],
) {
    // 가상 문단 → (구역, 본문 문단, 미주 경로). 경로 꼴은 `getEndnoteSourceAt` 의 `notePath` 와 같다.
    let owner = |para: usize| {
        let source = sources.get(para.checked_sub(base)?)?;
        let entry = |control_index, cell_index, cell_para_index| CellPathEntry {
            control_index,
            cell_index,
            cell_para_index,
            text_direction: 0,
        };
        let mut path = Vec::new();
        let mut control = source.control_index;
        for step in &source.cell_path {
            path.push(entry(control, step.cell_index, step.cell_para_index));
            control = step.control_index;
        }
        path.push(entry(control, 0, source.note_para_index));
        Some((source.section_index, source.para_index, path))
    };
    fn walk(
        node: &mut RenderNode,
        owner: &dyn Fn(usize) -> Option<(usize, usize, Vec<CellPathEntry>)>,
    ) {
        let RenderNode {
            node_type,
            container_path,
            ..
        } = &mut *node;
        let context = |sec: &mut Option<usize>, ctx: &mut CellContext| {
            let (section, host, mut path) = owner(ctx.parent_para_index)?;
            *sec = Some(section);
            path.append(&mut ctx.path);
            ctx.parent_para_index = host;
            ctx.path = path;
            Some(host)
        };
        let mut shape = |sec: &mut Option<usize>, para: &mut Option<usize>| {
            if let Some((section, host, mut path)) = para.and_then(owner) {
                *sec = Some(section);
                *para = Some(host);
                path.append(container_path);
                *container_path = path;
            }
        };
        match node_type {
            RenderNodeType::Image(img) => match img.cell_context.as_mut() {
                Some(ctx) => {
                    if let Some(host) = context(&mut img.section_index, ctx) {
                        img.para_index = Some(host);
                    }
                }
                None => {
                    if let Some((section, host, path)) = img.para_index.and_then(owner) {
                        img.section_index = Some(section);
                        img.para_index = Some(host);
                        img.cell_context = Some(CellContext {
                            in_textbox: false,
                            parent_para_index: host,
                            path,
                        });
                    }
                }
            },
            RenderNodeType::TextRun(run) => {
                if let Some(ctx) = run.cell_context.as_mut() {
                    context(&mut run.section_index, ctx);
                }
            }
            RenderNodeType::Table(table) => match table.cell_context.as_mut() {
                Some(ctx) => {
                    if let Some(host) = context(&mut table.section_index, ctx) {
                        table.para_index = Some(host);
                    }
                }
                None => {
                    // 표 노드의 경로는 마지막 마디가 표 자신이다(`nested_table_meta`).
                    if let (Some((section, host, mut path)), Some(control_index)) =
                        (table.para_index.and_then(owner), table.control_index)
                    {
                        path.push(CellPathEntry {
                            control_index,
                            cell_index: 0,
                            cell_para_index: 0,
                            text_direction: 0,
                        });
                        table.section_index = Some(section);
                        table.para_index = Some(host);
                        table.cell_context = Some(CellContext {
                            in_textbox: false,
                            parent_para_index: host,
                            path,
                        });
                    }
                }
            },
            // 미주 수식은 가상 문단 번호와 `note_ref` 로 이미 원본을 가리킨다. 그 번호는 두고
            // 쪽 개체 목록이 `cellPath` 로 낼 미주 경로만 단다(`parentParaIdx` 는 `note_ref`).
            RenderNodeType::Equation(eq) if eq.cell_index.is_none() => {
                if let Some((_, _, mut path)) = eq.para_index.and_then(owner) {
                    path.append(container_path);
                    *container_path = path;
                }
            }
            RenderNodeType::Rectangle(n) => shape(&mut n.section_index, &mut n.para_index),
            RenderNodeType::Ellipse(n) => shape(&mut n.section_index, &mut n.para_index),
            RenderNodeType::Line(n) => shape(&mut n.section_index, &mut n.para_index),
            RenderNodeType::Path(n) => shape(&mut n.section_index, &mut n.para_index),
            RenderNodeType::Group(n) => shape(&mut n.section_index, &mut n.para_index),
            _ => {}
        }
        for child in &mut node.children {
            walk(child, owner);
        }
    }
    walk(node, &owner);
}
