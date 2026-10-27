//! 렌더 트리 글 런의 위치를 캐럿·편집 API의 논리 축으로 옮긴다.
//!
//! 공식 렌더 트리의 `char_start`는 원문 글자 축이다. 글자겹침만 한 칸을 차지하고,
//! 글자처럼 취급한 개체와 각주·미주는 칸을 차지하지 않는다. 캐럿·편집 API는 이 개체들을
//! 각각 한 칸으로 센다(`text_to_logical_offset`). 캐럿 조회는 이 모듈이 옮긴 트리를 읽어
//! 렌더러가 그린 위치와 편집 오프셋을 같은 축에서 맞춘다. 렌더링과 공식 조회는 원래
//! 트리를 그대로 쓴다.

use std::collections::HashMap;

use crate::document_core::DocumentCore;
use crate::error::HwpError;
use crate::model::control::Control;
use crate::model::paragraph::Paragraph;
use crate::renderer::render_tree::{
    BoundingBox, PageLayoutContext, PageRenderTree, RenderNode, RenderNodeType,
};

/// 렌더 런 축의 각 칸이 논리 축에서 차지하는 자리와 논리 문단 길이.
struct AxisMap {
    logical: Vec<usize>,
    /// 렌더 런 칸의 원문 글자 인덱스. 글자겹침 칸은 원문 글자가 아니다.
    text: Vec<Option<usize>>,
    /// 논리 칸을 차지하는 컨트롤의 논리 위치(컨트롤 인덱스 → 위치).
    control_slots: HashMap<usize, usize>,
    logical_len: usize,
    /// 저장 줄 `i`의 원시 시작 앞에 놓인 논리 개체 수.
    objects_before_line: Vec<usize>,
}

fn axis_map(para: &Paragraph) -> AxisMap {
    let text_len = para.text.chars().count();
    let mut controls: Vec<(usize, usize, bool)> = para
        .controls
        .iter()
        .zip(para.control_text_positions())
        .enumerate()
        .filter(|(_, (control, _))| control.is_logical_inline())
        .map(|(index, (control, position))| {
            // 글자겹침만 렌더 런 축에서 한 칸을 차지한다.
            let overlap = matches!(control, Control::CharOverlap(co) if !co.chars.is_empty());
            (position.min(text_len), index, overlap)
        })
        .collect();
    controls.sort_unstable();
    let mut logical = Vec::with_capacity(text_len + controls.len());
    let mut text = Vec::with_capacity(text_len + controls.len());
    let mut control_slots = HashMap::new();
    let mut next = 0;
    let mut pending = controls.iter().peekable();
    for text_index in 0..=text_len {
        // 같은 자리의 개체는 글자 앞에 놓인다.
        while let Some(&(_, index, overlap)) = pending.next_if(|(at, _, _)| *at <= text_index) {
            control_slots.insert(index, next);
            if overlap {
                logical.push(next);
                text.push(None);
            }
            next += 1;
        }
        if text_index < text_len {
            logical.push(next);
            text.push(Some(text_index));
            next += 1;
        }
    }
    let control_raw = para.control_utf16_positions();
    let objects_before_line = para
        .line_segs
        .iter()
        .map(|line| {
            para.controls
                .iter()
                .zip(&control_raw)
                .filter(|(control, &at)| control.is_logical_inline() && at < line.text_start)
                .count()
        })
        .collect();
    AxisMap {
        logical,
        text,
        control_slots,
        logical_len: next,
        objects_before_line,
    }
}

/// 렌더 런 축 위치가 가리키는 원문 글자 인덱스. 글자겹침 칸이면 `None`이다.
pub(crate) fn run_axis_text_index(para: &Paragraph, index: usize) -> Option<usize> {
    axis_map(para).text.get(index).copied().flatten()
}

type ParaKey = (usize, usize, Vec<(usize, usize, usize)>);

fn para_key(run: &crate::renderer::render_tree::TextRunNode) -> Option<ParaKey> {
    let section = run.section_index?;
    match &run.cell_context {
        Some(context) => Some((
            section,
            context.parent_para_index,
            context
                .path
                .iter()
                .map(|entry| (entry.control_index, entry.cell_index, entry.cell_para_index))
                .collect(),
        )),
        None => Some((section, run.para_index?, Vec::new())),
    }
    // 머리말·꼬리말·주석 배치의 내부 표식은 문서 문단 좌표가 아니다.
    .filter(|(_, paragraph, _)| *paragraph < usize::MAX.saturating_sub(4096))
}

/// 한 런을 논리 축 런들로 바꾼다. 런 안에 개체 칸이 끼면 그 자리에서 나눈다.
fn split_run(
    node: &RenderNode,
    map: &AxisMap,
    textless_line: Option<Option<u32>>,
    ids: &mut PageLayoutContext,
) -> Option<Vec<RenderNode>> {
    let RenderNodeType::TextRun(run) = &node.node_type else {
        return None;
    };
    let start = run.char_start?;
    if run.char_overlap.is_some() || run.text.is_empty() {
        let logical = match textless_line.filter(|_| run.text.is_empty()) {
            // 글 없는 줄의 앵커는 줄 안 개체를 이미 센 위치다(공식 빈 줄 경로). 원문 글자
            // 축으로는 같은 자리의 개체 앞뒤를 가르지 못하므로 앞 줄의 개체 수만 더한다.
            Some(line) => {
                start
                    + line
                        .and_then(|line| map.objects_before_line.get(line as usize))
                        .copied()
                        .unwrap_or(0)
            }
            None if start >= map.logical.len() => map.logical_len,
            None => map.logical[start],
        };
        let mut node = node.clone();
        if let RenderNodeType::TextRun(run) = &mut node.node_type {
            run.char_start = Some(logical);
        }
        return Some(vec![node]);
    }
    let chars: Vec<char> = run.text.chars().collect();
    let slots = map.logical.get(start..start + chars.len())?;
    let positions = run.replay_positions_for(&run.text).into_owned();
    let mut pieces = Vec::new();
    let mut from = 0;
    while from < chars.len() {
        let mut to = from + 1;
        while to < chars.len() && slots[to] == slots[to - 1] + 1 {
            to += 1;
        }
        let mut piece = node.clone();
        if let RenderNodeType::TextRun(part) = &mut piece.node_type {
            part.char_start = Some(slots[from]);
            if from > 0 || to < chars.len() {
                let left = positions.get(from).copied().unwrap_or(0.0);
                let right = positions.get(to).copied().unwrap_or(node.bbox.width);
                part.text = chars[from..to].iter().collect();
                part.layout_positions = Some(
                    positions
                        .get(from..=to)
                        .map(|range| range.iter().map(|x| x - left).collect())
                        .unwrap_or_default(),
                );
                // 앞 조각은 문단 끝이 아니다. 뒤 조각만 원래 런의 끝 표식을 잇는다.
                part.is_para_end &= to == chars.len();
                part.is_line_break_end &= to == chars.len();
                piece.bbox = BoundingBox::new(
                    node.bbox.x + left,
                    node.bbox.y,
                    right - left,
                    node.bbox.height,
                );
                if from > 0 {
                    piece.id = ids.next_id();
                }
            }
        }
        pieces.push(piece);
        from = to;
    }
    Some(pieces)
}

fn convert(
    core: &DocumentCore,
    node: &mut RenderNode,
    maps: &mut HashMap<ParaKey, Option<AxisMap>>,
    ids: &mut PageLayoutContext,
) {
    // 머리말·꼬리말·주석 영역의 런은 본문 문단 좌표가 아니다.
    if matches!(
        node.node_type,
        RenderNodeType::Header | RenderNodeType::Footer | RenderNodeType::FootnoteArea
    ) {
        return;
    }
    let textless_line = match &node.node_type {
        RenderNodeType::TextLine(line)
            if !node.children.iter().any(|child| {
                matches!(&child.node_type, RenderNodeType::TextRun(run) if !run.text.is_empty())
            }) =>
        {
            Some(line.line_index)
        }
        _ => None,
    };
    let children = std::mem::take(&mut node.children);
    for mut child in children {
        let key = match &child.node_type {
            RenderNodeType::TextRun(run) if run.char_start.is_some() => para_key(run),
            _ => None,
        };
        let Some(key) = key else {
            convert(core, &mut child, maps, ids);
            node.children.push(child);
            continue;
        };
        let map = maps.entry(key.clone()).or_insert_with(|| {
            core.resolve_control_para(key.0, key.1, &key.2)
                .ok()
                .map(axis_map)
        });
        match map
            .as_ref()
            .and_then(|map| split_run(&child, map, textless_line, ids))
        {
            Some(pieces) => node.children.extend(pieces),
            None => node.children.push(child),
        }
    }
    order_note_markers(core, &mut node.children, maps);
}

/// 본문 문단의 주석 번호를 그 컨트롤의 논리 칸 순서로 같은 문단 런 사이에 놓는다.
/// 다른 형제 노드의 자리는 그대로 둔다.
fn order_note_markers(
    core: &DocumentCore,
    children: &mut [RenderNode],
    maps: &mut HashMap<ParaKey, Option<AxisMap>>,
) {
    let mut members: Vec<(usize, ParaKey, usize)> = Vec::new();
    for (index, child) in children.iter().enumerate() {
        match &child.node_type {
            RenderNodeType::TextRun(run) if run.cell_context.is_none() => {
                if let (Some(key), Some(start)) = (para_key(run), run.char_start) {
                    members.push((index, key, start));
                }
            }
            RenderNodeType::FootnoteMarker(marker) => {
                let key = (marker.section_index, marker.para_index, Vec::new());
                let map = maps.entry(key.clone()).or_insert_with(|| {
                    core.resolve_control_para(key.0, key.1, &key.2)
                        .ok()
                        .map(axis_map)
                });
                if let Some(slot) = map
                    .as_ref()
                    .and_then(|map| map.control_slots.get(&marker.control_index))
                {
                    members.push((index, key, *slot));
                }
            }
            _ => {}
        }
    }
    if !members.iter().any(|(index, _, _)| {
        matches!(
            children[*index].node_type,
            RenderNodeType::FootnoteMarker(_)
        )
    }) {
        return;
    }
    let slots: Vec<usize> = members.iter().map(|(index, _, _)| *index).collect();
    let mut sorted = members.clone();
    sorted.sort_by(|a, b| (&a.1, a.2).cmp(&(&b.1, b.2)));
    // 문단별 순서만 바꾼다. 서로 다른 문단의 노드 순서가 섞이면 정렬하지 않는다.
    if members.windows(2).any(|pair| pair[0].1 > pair[1].1) {
        return;
    }
    let moved: Vec<RenderNode> = sorted
        .iter()
        .map(|(index, _, _)| children[*index].clone())
        .collect();
    for (slot, node) in slots.into_iter().zip(moved) {
        children[slot] = node;
    }
}

impl DocumentCore {
    /// 글 런 위치를 캐럿·편집 API의 논리 축으로 옮긴 쪽 렌더 트리.
    pub(crate) fn logical_caret_tree(&self, mut tree: PageRenderTree) -> PageRenderTree {
        let PageRenderTree { root, frame } = &mut tree;
        convert(self, root, &mut HashMap::new(), frame);
        tree
    }

    /// 캐럿 빠른 경로가 따로 조판한 하위 트리도 같은 논리 축으로 옮긴다.
    pub(crate) fn logical_caret_node(&self, node: &mut RenderNode, ids: &mut PageLayoutContext) {
        convert(self, node, &mut HashMap::new(), ids);
    }

    /// 공개 렌더 트리(`build_page_render_tree`)를 캐럿·편집 API의 논리 축으로 옮긴다.
    pub fn build_page_render_tree_logical(
        &self,
        page_num: u32,
    ) -> Result<PageRenderTree, HwpError> {
        Ok(self.logical_caret_tree(self.build_page_render_tree(page_num)?))
    }

    pub(crate) fn build_page_tree_logical(
        &self,
        page_num: u32,
    ) -> Result<PageRenderTree, HwpError> {
        Ok(self.logical_caret_tree(self.build_page_tree(page_num)?))
    }

    pub(crate) fn build_page_tree_cached_logical(
        &self,
        page_num: u32,
    ) -> Result<PageRenderTree, HwpError> {
        Ok(self.logical_caret_tree(self.build_page_tree_cached(page_num)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::control::CharOverlap;

    #[test]
    fn overlap_takes_a_run_slot_and_notes_only_a_logical_slot() {
        let para = Paragraph {
            text: "앞뒤끝".into(),
            char_offsets: vec![0, 9, 18],
            controls: vec![
                Control::CharOverlap(CharOverlap {
                    chars: vec!['가'],
                    ..Default::default()
                }),
                Control::Footnote(Box::default()),
            ],
            ..Default::default()
        };
        assert_eq!(para.control_text_positions(), [1, 2]);
        let map = axis_map(&para);
        // 렌더 런 축: 앞 겹침 뒤 끝 / 논리 축: 앞 겹침 뒤 각주 끝
        assert_eq!(map.logical, [0, 1, 2, 4]);
        assert_eq!(map.text, [Some(0), None, Some(1), Some(2)]);
        assert_eq!(map.logical_len, 5);
    }
}
