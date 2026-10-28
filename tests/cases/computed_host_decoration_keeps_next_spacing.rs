//! 표와 글앞 개체를 함께 든 빈 호스트 뒤 문단의 앞 간격은 그 문단을 옮긴다.
//!
//! 글앞 선이 같은 호스트에 들어오면 format은 호스트의 빈 줄을 표 뒤 간격으로 옮긴다.
//! 계산 빈 호스트의 문단 끝을 앵커로 두면 다음 문단은 표 띠 안에서 시작하고, 앞 간격은
//! 표 띠에 흡수돼 아무 효과가 없었다. 선이 없을 때처럼 앞 간격만큼 내려가야 한다.

#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};

/// 빈 문서에 1x1 표를 넣어 HWPX로 저장하고 다시 연다. 다시 연 표의 호스트가 계산 빈 호스트다.
fn reopened_table_document() -> DocumentCore {
    let mut seed = DocumentCore::new_empty();
    seed.create_blank_document_native().expect("빈 문서");
    seed.create_table_native(0, 0, 0, 1, 1).expect("1x1 표");
    DocumentCore::from_bytes(&seed.export_hwpx_native().expect("HWPX 저장")).expect("다시 열기")
}

fn second_paragraph_y(doc: &DocumentCore) -> f64 {
    fn walk(node: &RenderNode, out: &mut Option<f64>) {
        if let RenderNodeType::TextRun(run) = &node.node_type {
            if out.is_none() && run.para_index == Some(1) && run.cell_context.is_none() {
                *out = Some(node.bbox.y);
            }
        }
        for child in &node.children {
            walk(child, out);
        }
    }
    let mut y = None;
    walk(&doc.build_page_render_tree(0).unwrap().root, &mut y);
    y.expect("둘째 문단 글줄")
}

/// 둘째 문단에 앞 간격(1500, 10px)을 주기 전과 뒤의 글줄 y 차이.
fn spacing_shift(with_line: bool) -> f64 {
    let mut doc = reopened_table_document();
    if with_line {
        doc.create_shape_control_native(
            0,
            0,
            0,
            6000,
            3000,
            15000,
            15000,
            false,
            "InFrontOfText",
            "line",
            false,
            false,
            &[],
        )
        .expect("글앞 선");
    }
    let before = second_paragraph_y(&doc);
    doc.apply_para_format_native(0, 1, r#"{"spacingBefore":1500}"#)
        .expect("앞 간격");
    second_paragraph_y(&doc) - before
}

#[test]
fn next_paragraph_spacing_moves_it_after_a_host_with_a_front_line() {
    let plain = spacing_shift(false);
    assert!(plain > 0.5, "선 없는 호스트 뒤 이동량 {plain}");
    let with_line = spacing_shift(true);
    // 문단은 표 띠 아래에서 시작하므로 앞 간격 10px만큼 그대로 내려간다.
    assert!(
        (with_line - 10.0).abs() < 0.2,
        "글앞 선이 있는 호스트 뒤 이동량 {with_line}"
    );
}
