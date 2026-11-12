//! 글상자 글 방향(`tbTextDirection`) 쓰기.
//!
//! 한/글 12.30 입력 > 개체 > 세로 글상자는 `textDirection="VERTICAL"` 글상자를 만들고,
//! 글 줄 길이는 상자 높이에서 위·아래 여백을 뺀 값이다(한컴 저장본 8504 HU 상자의
//! lineseg horzsize 7936). 종전 rhwp는 글상자 글 방향을 바꿀 길이 없었고, 세로 글상자에
//! 글을 넣어도 상자 너비로 줄을 나눴다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::control::Control;
use rhwp::model::shape::TextBox;
use rhwp::wasm_api::HwpDocument;

const TEXT: &str = "가나다라마바사아자차카타파하";

/// 빈 문서에 너비 8504·높이 20000 HU 떠 있는 글상자를 만들고 글을 넣는다.
fn doc_with_textbox() -> (HwpDocument, usize, usize) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    let result: serde_json::Value = serde_json::from_str(
        &doc.create_shape_control_native(
            0,
            0,
            0,
            8504,
            20000,
            1000,
            1000,
            false,
            "InFrontOfText",
            "textbox",
            false,
            false,
            &[],
        )
        .unwrap(),
    )
    .unwrap();
    let para = result["paraIdx"].as_u64().unwrap() as usize;
    let control = result["controlIdx"].as_u64().unwrap() as usize;
    doc.insert_text_in_cell_native(0, para, control, 0, 0, 0, TEXT)
        .unwrap();
    (doc, para, control)
}

fn text_box(doc: &HwpDocument, para: usize, control: usize) -> &TextBox {
    match &doc.document().sections[0].paragraphs[para].controls[control] {
        Control::Shape(shape) => shape.drawing().unwrap().text_box.as_ref().unwrap(),
        _ => panic!("글상자가 아니다"),
    }
}

fn direction(doc: &HwpDocument, para: usize, control: usize) -> String {
    let props: serde_json::Value =
        serde_json::from_str(&doc.get_shape_properties_native(0, para, control).unwrap()).unwrap();
    props["tbTextDirection"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn vertical_textbox_breaks_lines_by_box_height_and_round_trips() {
    let (mut doc, para, control) = doc_with_textbox();
    assert_eq!(direction(&doc, para, control), "Horizontal");
    let horizontal_lines = text_box(&doc, para, control).paragraphs[0].line_segs.len();
    assert!(
        horizontal_lines >= 2,
        "가로 글상자는 너비 8504로 줄을 나눈다"
    );

    doc.set_shape_properties_native(0, para, control, r#"{"tbTextDirection":"Vertical"}"#)
        .unwrap();
    assert_eq!(direction(&doc, para, control), "Vertical");
    let tb = text_box(&doc, para, control);
    assert_eq!(tb.list_attr & 0x07, 1);
    let lines = &tb.paragraphs[0].line_segs;
    assert_eq!(
        lines.len(),
        1,
        "14글자는 높이 20000 HU 한 열에 든다: {lines:?}"
    );
    assert!(
        (lines[0].segment_width - (20000 - 283 * 2)).abs() <= 4,
        "열 길이는 상자 높이 - 위·아래 여백: {}",
        lines[0].segment_width
    );
    assert_eq!(
        doc.get_cell_text_direction(0, para as u32, control as u32, 0)
            .unwrap(),
        1
    );

    // 세로 글상자에 더 친 글도 높이로 나눈다.
    doc.insert_text_in_cell_native(0, para, control, 0, 0, TEXT.chars().count(), TEXT)
        .unwrap();
    assert_eq!(
        text_box(&doc, para, control).paragraphs[0].line_segs.len(),
        2
    );

    for bytes in [doc.export_hwp().unwrap(), doc.export_hwpx().unwrap()] {
        let reopened = HwpDocument::from_bytes(&bytes).unwrap();
        assert_eq!(direction(&reopened, para, control), "Vertical");
    }

    doc.set_shape_properties_native(0, para, control, r#"{"tbTextDirection":"Horizontal"}"#)
        .unwrap();
    assert_eq!(direction(&doc, para, control), "Horizontal");
    assert!(text_box(&doc, para, control).paragraphs[0].line_segs.len() > 2);
}

#[test]
fn vertical_all_is_kept_in_hwpx() {
    let (mut doc, para, control) = doc_with_textbox();
    doc.set_shape_properties_native(0, para, control, r#"{"tbTextDirection":"VerticalAll"}"#)
        .unwrap();
    assert_eq!(direction(&doc, para, control), "VerticalAll");
    let reopened = HwpDocument::from_bytes(&doc.export_hwpx().unwrap()).unwrap();
    assert_eq!(direction(&reopened, para, control), "VerticalAll");
    assert_eq!(
        reopened
            .get_cell_text_direction(0, para as u32, control as u32, 0)
            .unwrap(),
        2
    );
}
