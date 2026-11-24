//! 쪽에서 나뉘는 행에 칸 선언 높이를 적은 표(한/글 «셀 높이를 같게» 결과와 같은 꼴).
//!
//! 쪽 끝에 놓은 2×2 표의 0행 첫 칸에 여덟 줄을 넣고 네 칸 모두에 같은 선언 높이를 준다.
//! 기준은 한/글 12.30(Mac)이 이 문서(HWP·HWPX)를 인쇄한 PDF의 가로 괘선이다.
//! - 12164HU(162.2px): 첫 조각은 두 줄 내용(38.4px)이 아니라 쪽 아래 닫는 선(1004.0)까지 그리고,
//!   다음 쪽 0행은 남은 여섯 줄(아래 259.8), 1행은 선언 높이(아래 422.1)다.
//! - 112964HU(1506.2px, 새 쪽 본문 876.8px보다 큼): 내용이 끝나도 선언 높이의 빈 띠를 쪽마다
//!   닫는 선에서 잘라 이어 그린다. 한/글은 5쪽(쪽 끝 표), 4쪽(첫 줄 표)으로 인쇄한다.
//!
//! 한/글이 이 두 문서를 다시 저장한 `samples/declared-row-page-band/hancom-{bottom,top}.hwp`
//! (첫 개체 상자 4295HU·63495HU)도 같은 쪽으로 인쇄한다.
//!
//! 수정 전 rhwp는 편집 중 첫 조각을 내용에서 닫았고(985.1), 새 쪽보다 큰 행은 다시 연 문서에서
//! 한 쪽에 통째로 두어 잘렸다(4쪽·3쪽). 한/글 저장본(첫 줄 표)은 0행을 첫 개체 상자에서 닫아
//! 3쪽이었다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

/// 본문 `body_lines`줄 뒤에 2×2 표를 넣고, 0행 첫 칸에 여덟 줄을 쓴 뒤 네 칸 높이를 `height`HU로 한다.
fn build(body_lines: usize, height: u32) -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    let core: &mut DocumentCore = &mut doc;
    for i in 0..body_lines {
        core.insert_text_native(0, i, 0, "본문").unwrap();
        if i + 1 < body_lines {
            core.split_paragraph_native(0, i, 2, None).unwrap();
        }
    }
    let made: serde_json::Value = serde_json::from_str(
        &core
            .create_table_native(0, body_lines - 1, 2, 2, 2)
            .unwrap(),
    )
    .unwrap();
    let para = made["paraIdx"].as_u64().unwrap() as usize;
    let ctrl = made["controlIdx"].as_u64().unwrap() as usize;
    for i in 0..8 {
        core.insert_text_in_cell_native(0, para, ctrl, 0, i, 0, "칸 글")
            .unwrap();
        if i < 7 {
            core.split_paragraph_in_cell_native(0, para, ctrl, 0, i, 3, None)
                .unwrap();
        }
    }
    for cell in 0..4 {
        core.set_cell_properties_native(0, para, ctrl, cell, &format!(r#"{{"height":{height}}}"#))
            .unwrap();
    }
    doc
}

/// 쪽마다 (행, 그 쪽에서 그 행 칸 상자의 아래) 목록.
fn row_bottoms(core: &DocumentCore) -> Vec<Vec<(u16, f64)>> {
    fn walk(node: &RenderNode, out: &mut Vec<(u16, f64)>) {
        if let RenderNodeType::TableCell(cell) = &node.node_type {
            let bottom = node.bbox.y + node.bbox.height;
            match out.iter_mut().find(|(row, _)| *row == cell.row) {
                Some(entry) => entry.1 = entry.1.max(bottom),
                None => out.push((cell.row, bottom)),
            }
        }
        for child in &node.children {
            walk(child, out);
        }
    }
    (0..core.page_count())
        .map(|page| {
            let tree = core.build_page_render_tree(page).unwrap();
            let mut out = Vec::new();
            walk(&tree.root, &mut out);
            out.sort_by_key(|(row, _)| *row);
            out
        })
        .collect()
}

fn assert_like_hancom(label: &str, core: &DocumentCore, hancom: &[&[(u16, f64)]]) {
    let pages = row_bottoms(core);
    assert_eq!(
        pages.len(),
        hancom.len(),
        "{label}: 한/글 인쇄와 같은 {}쪽이어야 한다, 실제 {pages:?}",
        hancom.len()
    );
    for (index, (actual, expected)) in pages.iter().zip(hancom).enumerate() {
        assert_eq!(
            actual.iter().map(|(row, _)| *row).collect::<Vec<_>>(),
            expected.iter().map(|(row, _)| *row).collect::<Vec<_>>(),
            "{label} {}쪽 행: {actual:?}",
            index + 1
        );
        for ((row, bottom), (_, want)) in actual.iter().zip(expected.iter()) {
            assert!(
                (bottom - want).abs() <= 0.5,
                "{label} {}쪽 {row}행 아래 = 한/글 {want:.2}px, 실제 {bottom:.2}px",
                index + 1
            );
        }
    }
}

fn check(body_lines: usize, height: u32, hancom: &[&[(u16, f64)]]) {
    let doc = build(body_lines, height);
    assert_like_hancom(&format!("{height}HU 편집 중"), &doc, hancom);
    for (format, bytes) in [
        ("hwp", doc.export_hwp_native().unwrap()),
        ("hwpx", doc.export_hwpx_native().unwrap()),
    ] {
        let reopened = DocumentCore::from_bytes(&bytes).unwrap();
        assert_like_hancom(
            &format!("{height}HU {format} 다시 연 문서"),
            &reopened,
            hancom,
        );
    }
}

fn sample(name: &str) -> DocumentCore {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples/declared-row-page-band")
        .join(name);
    DocumentCore::from_bytes(&std::fs::read(&path).unwrap()).unwrap()
}

const BOTTOM_112964: &[&[(u16, f64)]] = &[
    &[(0, 1004.0)],
    &[(0, 1004.0)],
    &[(0, 717.12), (1, 1004.0)],
    &[(1, 1004.0)],
    &[(1, 487.36)],
];
const TOP_112964: &[&[(u16, f64)]] = &[
    &[(0, 1004.0)],
    &[(0, 795.68), (1, 1004.0)],
    &[(1, 1004.0)],
    &[(1, 566.08)],
];

#[test]
fn split_row_with_declared_height_closes_first_piece_at_page_bottom() {
    check(38, 12164, &[&[(0, 1004.0)], &[(0, 259.84), (1, 422.08)]]);
}

#[test]
fn declared_row_taller_than_page_continues_on_following_pages() {
    check(38, 112964, BOTTOM_112964);
    check(1, 112964, TOP_112964);
}

#[test]
fn hancom_saved_declared_row_taller_than_page_continues_like_hancom() {
    assert_like_hancom(
        "한/글 저장본(쪽 끝 표)",
        &sample("hancom-bottom.hwp"),
        BOTTOM_112964,
    );
    assert_like_hancom(
        "한/글 저장본(첫 줄 표)",
        &sample("hancom-top.hwp"),
        TOP_112964,
    );
}
