//! 미주 내용 번호 속성 "작게": 미주 첫 문단 앞에 붙인 번호를 작은 위첨자로 그린다.
//!
//! 한컴 12.30은 작게면 미주 안의 번호를 본문 참조 번호처럼 작게 올려 그린다(화면 확인, 2026-11-11).
//! 미주 번호는 조판 전에 첫 문단 글자로 붙어(`prepend_endnote_marker_text`) 본문과 한 글 조각일 수 있어,
//! 배치가 끝난 첫 줄에서 번호 글자만 따로 조각 내 줄이고 뒤 조각을 그만큼 당긴다.

use super::super::render_tree::{RenderNode, RenderNodeType, TextRunNode};
use super::text_measurement::estimate_text_width;

/// 글 조각의 글자 경계(N+1개). 배치가 확정한 값이 있으면 그것을, 없으면 글자마다 잰 폭을 쓴다.
fn boundaries(run: &TextRunNode) -> Vec<f64> {
    let text = run.display_or_text();
    if let Some(positions) = &run.layout_positions {
        if positions.len() == text.chars().count() + 1 {
            return positions.clone();
        }
    }
    let mut out = vec![0.0];
    let mut prefix = String::new();
    for character in text.chars() {
        prefix.push(character);
        out.push(estimate_text_width(&prefix, &run.style));
    }
    out
}

/// 줄 `line`에서 문단 `para_index`의 앞 `chars` 글자를 작은 위첨자로 줄인다.
pub(super) fn shrink_leading_chars(line: &mut RenderNode, para_index: usize, chars: usize) {
    let mut index = 0;
    let mut shift = 0.0;
    while index < line.children.len() {
        if shift != 0.0 && matches!(line.children[index].node_type, RenderNodeType::TextRun(_)) {
            line.children[index].bbox.x -= shift;
        }
        // 줄일 글자가 든 조각인가: (조각 첫 글자 번호, 줄일 글자 수, 조각 글자 수)
        let target = match &line.children[index].node_type {
            RenderNodeType::TextRun(run)
                if run.para_index == Some(para_index)
                    && run.char_start.is_some_and(|start| start < chars)
                    && run.display_text.is_none()
                    && run.char_overlap.is_none() =>
            {
                let start = run.char_start.unwrap_or_default();
                let count = run.text.chars().count();
                Some((start, (chars - start).min(count), count))
            }
            _ => None,
        };
        let Some((start, take, count)) = target else {
            index += 1;
            continue;
        };
        if take < count {
            // 번호 글자와 뒤 글자를 두 조각으로 나눈다. 뒤 조각은 원래 경계를 그대로 쓴다.
            let mut rest = line.children[index].clone();
            let node = &mut line.children[index];
            let RenderNodeType::TextRun(run) = &mut node.node_type else {
                unreachable!()
            };
            let edges = boundaries(run);
            let split = edges[take];
            run.text = run.text.chars().take(take).collect();
            run.layout_positions = Some(edges[..=take].to_vec());
            run.is_para_end = false;
            run.is_line_break_end = false;
            node.bbox.width = split;
            if let RenderNodeType::TextRun(rest_run) = &mut rest.node_type {
                rest_run.text = rest_run.text.chars().skip(take).collect();
                rest_run.char_start = Some(start + take);
                rest_run.layout_positions = Some(edges[take..].iter().map(|x| x - split).collect());
            }
            rest.bbox.x += split;
            rest.bbox.width = (rest.bbox.width - split).max(0.0);
            line.children.insert(index + 1, rest);
        }
        let node = &mut line.children[index];
        let RenderNodeType::TextRun(run) = &mut node.node_type else {
            unreachable!()
        };
        let (size, baseline) =
            crate::renderer::script_metrics::ScriptMetricsContract::NoteReference.draw_metrics(
                run.style.font_size,
                run.baseline,
                true,
                false,
                run.style.font_position,
            );
        run.style.font_size = size;
        run.baseline = baseline;
        // 줄인 크기·기준선을 바로 썼으므로 그리기가 첨자를 다시 줄이지 않는다.
        run.style.superscript = false;
        run.style.subscript = false;
        run.style.font_position = None;
        run.layout_positions = None;
        let width = estimate_text_width(run.display_or_text(), &run.style);
        shift += node.bbox.width - width;
        node.bbox.width = width;
        index += 1;
    }
}
