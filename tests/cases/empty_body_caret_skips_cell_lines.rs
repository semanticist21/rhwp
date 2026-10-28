//! 빈 본문 문단의 캐럿은 그 문단의 줄에 놓인다.
//!
//! 빈 문단은 글자 런이 없어 `get_cursor_rect_native` 가 렌더 트리의 TextLine 으로
//! 자리를 찾는다. 표 칸의 줄도 칸 안 문단 번호를 `para_index` 로 가지므로, 앞쪽 표의
//! 칸 문단 1 줄이 본문 문단 1 보다 먼저 맞아 캐럿이 표 안에 그려졌다. 편집기는 그 자리에
//! 캐럿을 보이지만 글은 표 아래 본문 문단에 들어갔다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use serde_json::Value;

/// 첫 문단에 글 있는 칸 문단 셋을 담은 1×1 표, 그 뒤에 빈 본문 문단.
fn table_then_empty_paragraph() -> DocumentCore {
    let mut doc = DocumentCore::new_empty();
    {
        let mut section = rhwp::model::document::Section::default();
        section.section_def.page_def = rhwp::model::page::PageDef::a4_default();
        section
            .paragraphs
            .push(rhwp::model::paragraph::Paragraph::new_empty());
        let mut document = rhwp::model::document::Document::default();
        document.sections.push(section);
        doc.set_document(document);
    }
    doc.create_table_native(0, 0, 0, 1, 1).expect("1×1 표");
    for i in 0..3 {
        let text = format!("칸 문단 {}", i + 1);
        let len = text.chars().count();
        doc.insert_text_in_cell_native(0, 0, 0, 0, i, 0, &text)
            .expect("칸 문단 글 삽입");
        if i < 2 {
            doc.split_paragraph_in_cell_native(0, 0, 0, 0, i, len, None)
                .expect("칸 문단 쪼개기");
        }
    }
    doc
}

fn json(s: String) -> Value {
    serde_json::from_str(&s).unwrap()
}

#[test]
fn empty_body_paragraph_caret_is_not_drawn_on_a_cell_line() {
    let doc = table_then_empty_paragraph();
    let paragraphs = &doc.document().sections[0].paragraphs;
    let host = paragraphs
        .iter()
        .position(|p| p.controls.iter().any(|c| matches!(c, Control::Table(_))))
        .expect("표 문단");
    let body = host + 1;
    assert!(
        body < paragraphs.len() && paragraphs[body].text.is_empty(),
        "표 뒤에 빈 본문 문단이 있어야 한다 — 시험 설정 오류"
    );
    assert!(
        body < 3,
        "칸 문단 번호와 겹치는 본문 문단이어야 한다 — 시험 설정 오류"
    );

    let cell_last = json(
        doc.get_cursor_rect_in_cell_native(0, host, 0, 0, 2, 0)
            .unwrap(),
    );
    let caret = json(doc.get_cursor_rect_native(0, body, 0).unwrap());
    assert!(
        caret["y"].as_f64().unwrap() > cell_last["y"].as_f64().unwrap(),
        "표 뒤 빈 문단 캐럿이 표 칸 줄 위에 놓였다: caret {caret}, 칸 마지막 줄 {cell_last}"
    );

    // 캐럿 자리를 누르면 같은 본문 문단으로 가야 한다.
    let x = caret["x"].as_f64().unwrap();
    let y = caret["y"].as_f64().unwrap() + caret["height"].as_f64().unwrap() / 2.0;
    let hit = json(
        doc.hit_test_native(caret["pageIndex"].as_u64().unwrap() as u32, x, y)
            .unwrap(),
    );
    assert!(
        hit.get("parentParaIndex").is_none(),
        "캐럿 자리가 표 칸이다: {hit}"
    );
    assert_eq!(hit["paragraphIndex"].as_u64(), Some(body as u64));
}
