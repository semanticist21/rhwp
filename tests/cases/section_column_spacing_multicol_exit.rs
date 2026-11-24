//! 같은 쪽에서 단 정의가 바뀌는 모든 자리는 구역의 단 사이 간격을 쓰고, 다음 단 정의가 닫는 다단 글
//! 영역은 단마다 고르게 나눈다.
//!
//! 검사 유형: 관계형. 앞 영역 마지막 글줄 아래끝과 다음 영역 첫 상자 위끝의 간격, 구역 간격을 바꿀 때
//! 영역 첫 상자가 움직이는 양, 다단 영역의 단별 줄 수를 잰다.
//!
//! 한컴 12.30 Mac PDF 근거(2026-11-25):
//! - `samples/section_column_spacing/multicol-exit-5669.hwp`(rhwp가 만든 한 단·두 단·한 단·두 단·한 단,
//!   구역 단 사이 간격 5669): 두 단 마지막 줄 위끝 243.74pt → 다음 한 단 줄 위끝 310.46pt,
//!   줄 높이 9.96pt + 56.69pt. 간격 0·1134 판도 줄 높이 + 그 간격이다. 두 단 영역(11줄)은 6/5줄로 나뉜다
//!   (단 종류 배분 판도 같다).
//! - `samples/section_column_spacing/multicol-2to2-5669.hwp`(위 문서에서 가운데 한 단 문단을 지워 두 단
//!   영역 뒤에 새 두 단 정의가 바로 오는 판): 앞 두 단 마지막 줄 위끝 243.74pt → 다음 두 단 첫 줄 위끝
//!   310.46pt(줄 높이 + 56.69pt), 구역 간격 0 판은 187.10pt → 197.06pt(줄 높이만). 두 영역 모두 6/5줄이다.
//! - `samples/basic/shortcut.hwp`(구역 간격 2000)를 간격 0·5669로만 바꾼 판: 쪽마다 단 정의가 바뀌는
//!   자리(제목 → 헤더 띠, 헤더 띠 → 두 단 본문, 두 단 본문 → 헤더 띠, `<…>` 소제목 앞뒤)마다
//!   -20.04pt·+36.72pt씩 움직인다. 1쪽 `커서 이동` -20.04, `빈칸 삽입` -40.08, `지우기` -60.00,
//!   `뒤 글자 지우기` -80.04, 2쪽 `새 문서` -20.04, `<미리 보기 상태에서>` -39.96, `편집 용지` -60.00,
//!   `되돌리기` -99.96(간격 0). 원본은 7쪽, 간격 5669 판은 12쪽이다.
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

/// 본문 상자 (문단, 왼끝, 위끝, 아래끝).
type BodyBox = (usize, f64, f64, f64);

/// 쪽의 본문 글줄과 표. 표 안 글줄은 표가 대표한다.
fn boxes(core: &DocumentCore, page: u32) -> (Vec<BodyBox>, Vec<BodyBox>) {
    fn walk(n: &RenderNode, lines: &mut Vec<BodyBox>, tables: &mut Vec<BodyBox>) {
        let b = &n.bbox;
        match &n.node_type {
            RenderNodeType::Table(table) => {
                if let Some(pi) = table.para_index {
                    tables.push((pi, b.x, b.y, b.y + b.height));
                }
                return;
            }
            RenderNodeType::TextLine(line) => {
                if let Some(pi) = line.para_index {
                    lines.push((pi, b.x, b.y, b.y + b.height));
                }
            }
            _ => {}
        }
        for c in &n.children {
            walk(c, lines, tables);
        }
    }
    let tree = core.build_page_render_tree(page).expect("render tree");
    let (mut lines, mut tables) = (Vec::new(), Vec::new());
    walk(&tree.root, &mut lines, &mut tables);
    (lines, tables)
}

fn zone_bottom(lines: &[BodyBox], paras: std::ops::Range<usize>) -> f64 {
    lines
        .iter()
        .filter(|(pi, ..)| paras.contains(pi))
        .map(|(.., bottom)| *bottom)
        .fold(f64::NEG_INFINITY, f64::max)
}

fn top_of(boxes: &[BodyBox], pi: usize) -> Option<f64> {
    boxes
        .iter()
        .filter(|(p, ..)| *p == pi)
        .map(|(_, _, top, _)| *top)
        .reduce(f64::min)
}

/// 문단 줄의 단별 수(왼쪽 단부터).
fn column_line_counts(lines: &[BodyBox], pi: usize) -> Vec<usize> {
    let mut columns: Vec<(f64, usize)> = Vec::new();
    for (_, x, ..) in lines.iter().filter(|(p, ..)| *p == pi) {
        match columns
            .iter_mut()
            .find(|(left, _)| (*left - *x).abs() < 40.0)
        {
            Some((_, count)) => *count += 1,
            None => columns.push((*x, 1)),
        }
    }
    columns.sort_by(|a, b| a.0.total_cmp(&b.0));
    columns.into_iter().map(|(_, count)| count).collect()
}

/// rhwp가 만든 문서: 문단 1·3이 두 단, 문단 0·2·4가 한 단이다.
fn exit_gaps(spacing: Option<i32>) -> [f64; 2] {
    let core = load(
        "samples/section_column_spacing/multicol-exit-5669.hwp",
        spacing,
    );
    let (lines, _) = boxes(&core, 0);
    [
        top_of(&lines, 2).unwrap() - zone_bottom(&lines, 1..2),
        top_of(&lines, 4).unwrap() - zone_bottom(&lines, 3..4),
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

#[test]
fn zone_closed_by_next_column_definition_is_balanced() {
    let core = load(
        "samples/section_column_spacing/multicol-exit-5669.hwp",
        None,
    );
    let (lines, _) = boxes(&core, 0);
    for pi in [1, 3] {
        assert_eq!(
            column_line_counts(&lines, pi),
            [6, 5],
            "다음 단 정의가 닫는 두 단 영역(문단 {pi}, 11줄)은 한컴처럼 6/5줄로 나뉘어야 한다"
        );
    }
}

/// 두 단 영역(문단 1) 바로 뒤에 새 두 단 정의(문단 2)가 오고 문단 3이 한 단이다.
#[test]
fn two_column_zone_after_two_column_zone_uses_section_column_spacing() {
    for spacing in [None, Some(0)] {
        let core = load(
            "samples/section_column_spacing/multicol-2to2-5669.hwp",
            spacing,
        );
        let (lines, _) = boxes(&core, 0);
        let expected = hu_px(spacing.unwrap_or(5669));
        let gap = top_of(&lines, 2).unwrap() - zone_bottom(&lines, 1..2);
        assert!(
            (gap - expected).abs() <= TOL_PX,
            "두 단 → 두 단 간격 {gap:.2}px 는 구역 단 사이 간격 {expected:.2}px 여야 한다 ({spacing:?})"
        );
        for pi in [1, 2] {
            assert_eq!(
                column_line_counts(&lines, pi),
                [6, 5],
                "문단 {pi} ({spacing:?})"
            );
        }
    }
}

/// shortcut.hwp 1쪽: 문단 2~28 두 단 본문 → 문단 29 한 단 헤더 띠(`지우기`).
fn shortcut_exit_gap(spacing: Option<i32>) -> f64 {
    let core = load("samples/basic/shortcut.hwp", spacing);
    let (lines, tables) = boxes(&core, 0);
    top_of(&tables, 29).unwrap() - zone_bottom(&lines, 2..29)
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

/// shortcut.hwp의 (쪽, 문단, 표인지, 같은 쪽에서 그 앞에 있는 단 정의 전환 수).
const SHORTCUT_ZONE_STARTS: [(u32, usize, bool, i32); 9] = [
    (0, 1, true, 1),   // 제목 → 헤더 띠 `커서 이동`
    (0, 2, false, 2),  // 헤더 띠 → 두 단 본문 `빈칸 삽입`
    (0, 29, true, 3),  // 두 단 본문 → 헤더 띠 `지우기`
    (0, 30, false, 4), // 헤더 띠 → 두 단 본문 `뒤 글자 지우기`
    (1, 37, false, 1), // 쪽 첫 헤더 띠 → 두 단 본문 `새 문서`
    (1, 47, false, 2), // 두 단 본문 → `<미리 보기 상태에서>`
    (1, 48, false, 3), // `<…>` 소제목 → 두 단 본문 `편집 용지`
    (1, 57, true, 4),  // 두 단 본문 → 헤더 띠 `편`
    (1, 58, false, 5), // 헤더 띠 → 두 단 본문 `되돌리기`
];

fn shortcut_zone_tops(spacing: Option<i32>) -> Vec<f64> {
    let core = load("samples/basic/shortcut.hwp", spacing);
    SHORTCUT_ZONE_STARTS
        .iter()
        .map(|&(page, pi, table, _)| {
            let (lines, tables) = boxes(&core, page);
            top_of(if table { &tables } else { &lines }, pi)
                .unwrap_or_else(|| panic!("{}쪽 문단 {pi} ({spacing:?})", page + 1))
        })
        .collect()
}

#[test]
fn shortcut_every_zone_transition_follows_section_column_spacing() {
    let base = shortcut_zone_tops(None);
    let moved = shortcut_zone_tops(Some(0));
    for ((&(page, pi, _, transitions), base), moved) in
        SHORTCUT_ZONE_STARTS.iter().zip(&base).zip(&moved)
    {
        let expected = hu_px(-2000) * f64::from(transitions);
        assert!(
            (moved - base - expected).abs() <= TOL_PX,
            "구역 간격 2000 → 0 에서 {}쪽 문단 {pi} 가 {:.2}px 움직였다. 한컴처럼 단 정의 전환 {transitions}번 몫 {expected:.2}px 여야 한다",
            page + 1,
            moved - base
        );
    }
}

#[test]
fn shortcut_page_count_follows_section_column_spacing() {
    for (spacing, pages) in [(None, 7), (Some(5669), 12)] {
        assert_eq!(
            load("samples/basic/shortcut.hwp", spacing).page_count(),
            pages,
            "구역 간격 {spacing:?}"
        );
    }
}
