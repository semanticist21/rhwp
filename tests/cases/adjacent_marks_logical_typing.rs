//! 관계형. 한 글자 자리에 맞붙은 각주 사이에 친 글은 그 사이에 남는다.
//! `insertTextLogical`의 논리 자리 `앞(0) [각주](1) [각주](2) 뒤(3)`에서 2는 두 각주 사이다.
//! 종전에는 같은 글자 자리로 바꿔 글을 두 각주 뒤에 넣었다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::model::control::Control;
use rhwp::wasm_api::HwpDocument;

/// 각주 컨트롤의 글자 자리.
fn note_positions(doc: &HwpDocument) -> Vec<usize> {
    let para = &doc.document().sections[0].paragraphs[0];
    let positions: Vec<usize> =
        serde_json::from_str(&doc.get_control_text_positions(0, 0)).unwrap();
    para.controls
        .iter()
        .zip(positions)
        .filter(|(ctrl, _)| matches!(ctrl, Control::Footnote(_)))
        .map(|(_, pos)| pos)
        .collect()
}

fn two_notes() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, "앞뒤").unwrap();
    doc.insert_footnote_native(0, 0, 1).unwrap();
    doc.insert_footnote_native(0, 0, 1).unwrap();
    assert_eq!(note_positions(&doc), vec![1, 1]);
    doc
}

#[test]
fn typing_between_adjacent_notes_stays_between() {
    let mut doc = two_notes();
    let result = doc.insert_text_logical(0, 0, 2, "사이").unwrap();
    assert!(result.contains(r#""logicalOffset":4"#), "{result}");
    assert_eq!(doc.document().sections[0].paragraphs[0].text, "앞사이뒤");
    assert_eq!(note_positions(&doc), vec![1, 3]);

    for bytes in [
        doc.export_hwp_native().unwrap(),
        doc.export_hwpx_native().unwrap(),
    ] {
        let reopened = HwpDocument::from_bytes(&bytes).unwrap();
        assert_eq!(
            reopened.document().sections[0].paragraphs[0].text,
            "앞사이뒤"
        );
        assert_eq!(note_positions(&reopened), vec![1, 3]);
    }
}

#[test]
fn typing_before_and_after_adjacent_notes_keeps_sides() {
    let mut doc = two_notes();
    doc.insert_text_logical(0, 0, 1, "전").unwrap();
    assert_eq!(doc.document().sections[0].paragraphs[0].text, "앞전뒤");
    assert_eq!(note_positions(&doc), vec![2, 2]);
    // 앞(0) 전(1) [각주](2) [각주](3) 뒤(4): 4는 두 각주 뒤.
    doc.insert_text_logical(0, 0, 4, "후").unwrap();
    assert_eq!(doc.document().sections[0].paragraphs[0].text, "앞전후뒤");
    assert_eq!(note_positions(&doc), vec![2, 2]);
}
