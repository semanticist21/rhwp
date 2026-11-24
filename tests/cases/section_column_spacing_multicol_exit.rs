//! 다단 영역에서 한 단 영역으로 바뀔 때도 구역의 단 사이 간격을 쓴다.
//!
//! 검사 유형: 관계형. 앞 다단 영역 마지막 글줄 아래끝과 다음 한 단 영역 첫 상자 위끝의 간격을 잰다.
//!
//! 한컴 12.30 Mac PDF 근거(2026-11-25):
//! - `samples/section_column_spacing/multicol-exit-5669.hwp`(rhwp가 만든 한 단·두 단·한 단·두 단·한 단,
//!   구역 단 사이 간격 5669): 두 단 마지막 줄 위끝 243.74pt → 다음 한 단 줄 위끝 310.46pt,
//!   줄 높이 9.96pt + 56.69pt. 간격 0·1134 판도 줄 높이 + 그 간격이다.
//! - `samples/basic/shortcut.hwp`(구역 간격 2000)를 간격 0·5669로만 바꾼 판: 쪽마다 단 정의가 바뀌는
//!   자리(두 단 본문 → 한 단 헤더 띠 포함)마다 -20.04pt·+36.72pt씩 움직인다. 쪽 수는 7쪽 그대로다(원본).
#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;

use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::DocumentCore;

const TOL_PX: f64 = 0.5;

fn hu_px(hu: i32) -> f64 {
    f64::from(hu) * 96.0 / 7200.0
}

fn load(sample: &str, spacing: Option<i32>) -> DocumentCore {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(sample);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut core = DocumentCore::from_bytes(&bytes).expect("문서 로드");
    if let Some(spacing) = spacing {
        core.set_section_def_native(0, &format!(r#"{{"columnSpacing":{spacing}}}"#))
            .expect("구역 단 사이 간격");
    }
    core
}

/// 1쪽 본문 글줄과 표의 (문단, 위끝, 아래끝). 표 안 글줄은 표가 대표한다.
fn boxes(core: &DocumentCore) -> (Vec<(usize, f64, f64)>, Vec<(usize, f64, f64)>) {
    fn walk(
        n: &RenderNode,
        lines: &mut Vec<(usize, f64, f64)>,
        tables: &mut Vec<(usize, f64, f64)>,
    ) {
        let span = (n.bbox.y, n.bbox.y + n.bbox.height);
        match &n.node_type {
            RenderNodeType::Table(table) => {
                if let Some(pi) = table.para_index {
                    tables.push((pi, span.0, span.1));
                }
                return;
            }
            RenderNodeType::TextLine(line) => {
                if let Some(pi) = line.para_index {
                    lines.push((pi, span.0, span.1));
                }
            }
            _ => {}
        }
        for c in &n.children {
            walk(c, lines, tables);
        }
    }
    let tree = core.build_page_render_tree(0).expect("1쪽 render tree");
    let (mut lines, mut tables) = (Vec::new(), Vec::new());
    walk(&tree.root, &mut lines, &mut tables);
    (lines, tables)
}

fn zone_bottom(lines: &[(usize, f64, f64)], paras: std::ops::Range<usize>) -> f64 {
    lines
        .iter()
        .filter(|(pi, _, _)| paras.contains(pi))
        .map(|(_, _, bottom)| *bottom)
        .fold(f64::NEG_INFINITY, f64::max)
}

fn top_of(boxes: &[(usize, f64, f64)], pi: usize) -> f64 {
    boxes
        .iter()
        .filter(|(p, _, _)| *p == pi)
        .map(|(_, top, _)| *top)
        .fold(f64::INFINITY, f64::min)
}

/// rhwp가 만든 문서: 문단 1·3이 두 단, 문단 0·2·4가 한 단이다.
fn exit_gaps(spacing: Option<i32>) -> [f64; 2] {
    let core = load(
        "samples/section_column_spacing/multicol-exit-5669.hwp",
        spacing,
    );
    let (lines, _) = boxes(&core);
    [
        top_of(&lines, 2) - zone_bottom(&lines, 1..2),
        top_of(&lines, 4) - zone_bottom(&lines, 3..4),
    ]
}

#[test]
fn multicol_to_single_uses_section_column_spacing() {
    for spacing in [None, Some(0), Some(1134)] {
        let expected = hu_px(spacing.unwrap_or(5669));
        for gap in exit_gaps(spacing) {
            assert!(
                (gap - expected).abs() <= TOL_PX,
                "두 단 → 한 단 간격 {gap:.2}px 는 구역 단 사이 간격 {expected:.2}px 여야 한다 ({spacing:?})"
            );
        }
    }
}

/// shortcut.hwp 1쪽: 문단 2~28 두 단 본문 → 문단 29 한 단 헤더 띠(`지우기`).
fn shortcut_exit_gap(spacing: Option<i32>) -> f64 {
    let core = load("samples/basic/shortcut.hwp", spacing);
    let (lines, tables) = boxes(&core);
    top_of(&tables, 29) - zone_bottom(&lines, 2..29)
}

#[test]
fn shortcut_multicol_exit_follows_section_column_spacing() {
    let base = shortcut_exit_gap(None);
    for spacing in [0, 5669] {
        let moved = shortcut_exit_gap(Some(spacing)) - base;
        let expected = hu_px(spacing - 2000);
        assert!(
            (moved - expected).abs() <= TOL_PX,
            "구역 간격 2000 → {spacing} 에서 헤더 띠가 {moved:.2}px 움직였다. 한컴처럼 {expected:.2}px 여야 한다"
        );
    }
}
