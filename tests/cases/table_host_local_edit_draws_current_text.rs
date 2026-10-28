//! 표만 든 빈 문단의 쪽 안 입력은 쪽 나눔이 수용한 옛 호스트 구성을 그리지 않는다.
//!
//! 표의 호스트 문단은 쪽 나눔이 확정한 구성을 쪽에 싣고 배치가 그것을 먼저 쓴다.
//! 쪽 안 입력(`replace_body_text_local_native`)이 쪽 나눔을 미루면 그 문단에 첫 글자만
//! 그려지고 캐럿이 줄 처음에 남았다. 입력 뒤의 글과 캐럿은 전체 쪽 나눔과 같아야 한다.

#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};

fn blank() -> DocumentCore {
    let mut doc = DocumentCore::new_empty();
    let mut section = rhwp::model::document::Section::default();
    section.section_def.page_def = rhwp::model::page::PageDef::a4_default();
    section
        .paragraphs
        .push(rhwp::model::paragraph::Paragraph::new_empty());
    let mut document = rhwp::model::document::Document::default();
    document.sections.push(section);
    doc.set_document(document);
    doc
}

fn host_text(node: &RenderNode, out: &mut String) {
    if let RenderNodeType::TextRun(run) = &node.node_type {
        if run.para_index == Some(1) && run.cell_context.is_none() {
            out.push_str(&run.text);
        }
    }
    for child in &node.children {
        host_text(child, out);
    }
}

fn caret_x(doc: &DocumentCore, offset: usize) -> f64 {
    let rect: serde_json::Value =
        serde_json::from_str(&doc.get_cursor_rect_native(0, 1, offset).unwrap()).unwrap();
    rect["x"].as_f64().unwrap()
}

#[test]
fn local_edit_in_table_only_paragraph_draws_current_text() {
    let mut seed = blank();
    seed.insert_text_native(0, 0, 0, "표 앞 문단").unwrap();
    seed.create_table_native(0, 0, 6, 2, 2).unwrap();
    // 저장본을 다시 열어 표만 든 둘째 문단을 쪽 나눔이 계산한 호스트로 만든다.
    let mut doc = DocumentCore::from_bytes(&seed.export_hwp_native().unwrap()).unwrap();
    assert!(doc.document().sections[0].paragraphs[1].text.is_empty());

    doc.insert_text_native(0, 1, 0, "ㅎ").unwrap();
    for text in ["하", "한"] {
        doc.replace_body_text_local_native(0, 1, 0, 1, text)
            .unwrap();
    }
    doc.replace_body_text_local_native(0, 1, 1, 0, "ㄱ")
        .unwrap();
    for text in ["그", "글"] {
        doc.replace_body_text_local_native(0, 1, 1, 1, text)
            .unwrap();
    }

    let mut drawn = String::new();
    host_text(&doc.build_page_render_tree(0).unwrap().root, &mut drawn);
    assert_eq!(drawn, "한글");
    // 둘째 글자 뒤 캐럿은 첫 글자 뒤보다 오른쪽이다.
    assert!(caret_x(&doc, 2) > caret_x(&doc, 1));
}
