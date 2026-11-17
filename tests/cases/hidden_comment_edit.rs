//! 숨은 설명 넣기·지우기와 따로 문서로 편집하기.
//!
//! 한컴 12.30 Mac 입력 > 주석 > 숨은 설명은 캐럿 자리에 숨은 설명을 넣고 "<<숨은 설명>>" 화면에서 고친다.
//! 그 화면의 문단은 같은 서식표의 따로 문서로 꺼내 고치고, 되돌릴 때 새 글자 모양을 이 문서 서식표로 옮긴다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::model::control::Control;
use rhwp::model::paragraph::Paragraph;
use rhwp::wasm_api::HwpDocument;

fn blank() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc
}

fn comments(paragraphs: &[Paragraph]) -> Vec<&Vec<Paragraph>> {
    paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .filter_map(|c| match c {
            Control::HiddenComment(h) => Some(&h.paragraphs),
            _ => None,
        })
        .collect()
}

fn reopen(bytes: &[u8]) -> HwpDocument {
    HwpDocument::from_bytes(bytes).unwrap()
}

#[test]
fn hidden_comment_is_inserted_edited_apart_and_saved() {
    let mut doc = blank();
    doc.insert_text_native(0, 0, 0, "body").unwrap();
    let result = doc.insert_hidden_comment_native(0, 0, &[], 4).unwrap();
    assert!(result.contains("\"controlIdx\":"), "{result}");
    let body = &doc.document().sections[0].paragraphs[0];
    assert_eq!(body.text, "body", "본문 글은 그대로다");
    assert_eq!(comments(&doc.document().sections[0].paragraphs).len(), 1);

    let control = doc.document().sections[0].paragraphs[0]
        .controls
        .iter()
        .position(|c| matches!(c, Control::HiddenComment(_)))
        .unwrap();
    // 따로 문서에서 글을 치고 굵게 한다. 굵은 글자 모양은 그 문서 서식표에 새로 생긴다.
    let mut sub = HwpDocument::from(doc.hidden_comment_document(0, 0, &[], control).unwrap());
    sub.insert_text_native(0, 0, 0, "secret").unwrap();
    sub.apply_char_format_native(0, 0, 0, 6, r#"{"bold":true}"#)
        .unwrap();
    let before_shapes = doc.document().doc_info.char_shapes.len();
    doc.set_hidden_comment_from(0, 0, &[], control, sub.document())
        .unwrap();

    let check = |doc: &HwpDocument, label: &str| {
        let list = comments(&doc.document().sections[0].paragraphs);
        assert_eq!(list.len(), 1, "{label}: 숨은 설명 하나");
        let para = &list[0][0];
        assert_eq!(para.text, "secret", "{label}");
        let shape = para.char_shapes[0].char_shape_id as usize;
        assert!(
            doc.document().doc_info.char_shapes[shape].bold,
            "{label}: 숨은 설명 글이 이 문서 서식표의 굵은 모양을 가리킨다"
        );
        assert_eq!(
            doc.document().sections[0].paragraphs[0].text,
            "body",
            "{label}"
        );
    };
    check(&doc, "편집 뒤");
    assert!(doc.document().doc_info.char_shapes.len() > before_shapes);
    check(&reopen(&doc.export_hwp_native().unwrap()), "HWP 다시 열기");
    check(
        &reopen(&doc.export_hwpx_native().unwrap()),
        "HWPX 다시 열기",
    );

    doc.delete_hidden_comment_native(0, 0, &[], control)
        .unwrap();
    assert!(comments(&doc.document().sections[0].paragraphs).is_empty());
    let saved = reopen(&doc.export_hwp_native().unwrap());
    assert!(comments(&saved.document().sections[0].paragraphs).is_empty());
    assert_eq!(saved.document().sections[0].paragraphs[0].text, "body");
}

#[test]
fn hidden_comment_goes_into_a_table_cell_by_path() {
    let mut doc = blank();
    let table: serde_json::Value =
        serde_json::from_str(&doc.create_table_native(0, 0, 0, 1, 1).unwrap()).unwrap();
    let para = table["paraIdx"].as_u64().unwrap() as usize;
    let ctrl = table["controlIdx"].as_u64().unwrap() as usize;
    let path = [(ctrl, 0, 0)];
    doc.insert_hidden_comment_native(0, para, &path, 0).unwrap();
    let cell = |doc: &HwpDocument| match &doc.document().sections[0].paragraphs[para].controls[ctrl]
    {
        Control::Table(t) => t.cells[0].paragraphs.clone(),
        _ => panic!("표"),
    };
    assert_eq!(comments(&cell(&doc)).len(), 1);
    let sub = HwpDocument::from(doc.hidden_comment_document(0, para, &path, 0).unwrap());
    assert_eq!(sub.document().sections[0].paragraphs.len(), 1);
    let saved = reopen(&doc.export_hwpx_native().unwrap());
    assert_eq!(comments(&cell(&saved)).len(), 1, "HWPX 다시 열기");
    doc.delete_hidden_comment_native(0, para, &path, 0).unwrap();
    assert!(comments(&cell(&doc)).is_empty());
}

#[test]
fn hidden_comment_code_is_shown_with_control_codes() {
    let mut doc = blank();
    doc.insert_text_native(0, 0, 0, "ab").unwrap();
    doc.insert_hidden_comment_native(0, 0, &[], 1).unwrap();
    doc.set_show_control_codes(true);
    let layout = doc.get_page_text_layout_native(0).unwrap();
    assert!(layout.contains("[숨은 설명]"), "{layout}");
}
