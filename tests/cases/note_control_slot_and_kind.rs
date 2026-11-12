//! 맞붙은 부호 자리의 주석·개체 넣기와 각주·미주 변환.
//!
//! 한/글 12.30 Mac: `앞[각주]뒤`에서 각주 바로 앞 캐럿에 넣은 각주는 그 각주 앞(새 1번)에,
//! 바로 뒤 캐럿에 넣은 각주는 그 뒤에 들어간다. 글자 자리를 받는 넣기 API는 그 자리 부호 모두
//! 뒤에 넣으므로 `placeControlInSlot`이 캐럿의 논리 자리로 옮긴다.
//! 각주/미주 변환은 주석 내용·자리를 두고 종류·번호·'각주'↔'미주' 스타일만 바꾼다(한컴 저장본).
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::control::{AutoNumberType, Control};
use rhwp::model::paragraph::Paragraph;
use rhwp::wasm_api::HwpDocument;

/// 주석 차례대로 (종류, 번호, 내용).
fn notes(doc: &HwpDocument) -> Vec<(&'static str, u16, String)> {
    let text = |paragraphs: &[Paragraph]| -> String {
        paragraphs
            .iter()
            .map(|para| para.text.trim().to_string())
            .collect()
    };
    doc.document().sections[0].paragraphs[0]
        .controls
        .iter()
        .filter_map(|ctrl| match ctrl {
            Control::Footnote(note) => Some(("fn", note.number, text(&note.paragraphs))),
            Control::Endnote(note) => Some(("en", note.number, text(&note.paragraphs))),
            Control::Equation(_) => Some(("eq", 0, String::new())),
            _ => None,
        })
        .collect()
}

/// `앞[각주 가]뒤` — 각주는 글자 자리 1에 있다.
fn one_note() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, "앞뒤").unwrap();
    doc.insert_footnote_native(0, 0, 1).unwrap();
    doc.insert_text_in_footnote_native(0, 0, 2, 0, 2, "가")
        .unwrap();
    doc
}

fn control_index(result: &str) -> usize {
    let value: serde_json::Value = serde_json::from_str(result).unwrap();
    value["controlIdx"].as_u64().unwrap() as usize
}

/// 캐럿 논리 자리를 (글자 자리, 그 자리에서 지나온 개체 수)로 바꾼다 — 앱이 쓰는 계산.
fn slot(doc: &HwpDocument, logical: u32) -> (usize, usize) {
    let text = doc.logical_to_text_offset(0, 0, logical).unwrap();
    let base = doc.text_to_logical_offset(0, 0, text).unwrap();
    (text as usize, (logical - base) as usize)
}

#[test]
fn note_inserted_right_before_a_note_lands_before_it() {
    let mut doc = one_note();
    let (text, before) = slot(&doc, 1);
    assert_eq!((text, before), (1, 0));
    let inserted = control_index(&doc.insert_footnote_native(0, 0, text).unwrap());
    let placed = doc
        .place_control_in_slot_native(0, 0, inserted, before)
        .unwrap();
    assert_eq!(control_index(&placed), inserted - 1);
    doc.insert_text_in_footnote_native(0, 0, control_index(&placed), 0, 2, "새")
        .unwrap();
    assert_eq!(
        notes(&doc),
        vec![("fn", 1, "새".to_string()), ("fn", 2, "가".to_string())]
    );
}

#[test]
fn note_inserted_right_after_a_note_stays_after_it() {
    let mut doc = one_note();
    let (text, before) = slot(&doc, 2);
    assert_eq!((text, before), (1, 1));
    let inserted = control_index(&doc.insert_footnote_native(0, 0, text).unwrap());
    let placed = doc
        .place_control_in_slot_native(0, 0, inserted, before)
        .unwrap();
    assert_eq!(control_index(&placed), inserted);
    assert_eq!(notes(&doc)[0], ("fn", 1, "가".to_string()));
}

#[test]
fn equation_between_two_notes_lands_between_them() {
    let mut doc = one_note();
    doc.insert_footnote_native(0, 0, 1).unwrap();
    doc.insert_text_in_footnote_native(0, 0, 3, 0, 2, "나")
        .unwrap();
    let (text, before) = slot(&doc, 2);
    assert_eq!((text, before), (1, 1));
    let inserted = control_index(
        &doc.insert_equation_native(0, 0, text, "x", 1000, 0)
            .unwrap(),
    );
    doc.place_control_in_slot_native(0, 0, inserted, before)
        .unwrap();
    let kinds: Vec<_> = notes(&doc)
        .into_iter()
        .map(|(kind, number, _)| (kind, number))
        .collect();
    assert_eq!(kinds, vec![("fn", 1), ("eq", 0), ("fn", 2)]);
    for bytes in [doc.export_hwp().unwrap(), doc.export_hwpx().unwrap()] {
        let reopened = HwpDocument::from_bytes(&bytes).unwrap();
        let kinds: Vec<_> = notes(&reopened)
            .into_iter()
            .map(|(kind, number, _)| (kind, number))
            .collect();
        assert_eq!(kinds, vec![("fn", 1), ("eq", 0), ("fn", 2)]);
    }
}

/// `앞[각주 가][각주 나]뒤 끝 미[미주 미]주`.
fn notes_doc() -> HwpDocument {
    let mut doc = one_note();
    doc.insert_footnote_native(0, 0, 1).unwrap();
    doc.insert_text_in_footnote_native(0, 0, 3, 0, 2, "나")
        .unwrap();
    doc.insert_text_native(0, 0, 2, " 끝 미주").unwrap();
    doc.insert_endnote_native(0, 0, 6).unwrap();
    doc.insert_text_in_footnote_native(0, 0, 4, 0, 2, "미")
        .unwrap();
    assert_eq!(
        notes(&doc),
        vec![
            ("fn", 1, "가".to_string()),
            ("fn", 2, "나".to_string()),
            ("en", 1, "미".to_string()),
        ]
    );
    doc
}

/// 주석 문단의 (스타일, 번호 종류).
fn note_paragraph_kinds(doc: &HwpDocument) -> Vec<(u8, Vec<AutoNumberType>)> {
    doc.document().sections[0].paragraphs[0]
        .controls
        .iter()
        .filter_map(|ctrl| match ctrl {
            Control::Footnote(note) => Some(&note.paragraphs),
            Control::Endnote(note) => Some(&note.paragraphs),
            _ => None,
        })
        .map(|paragraphs| {
            let para = &paragraphs[0];
            let kinds = para
                .controls
                .iter()
                .filter_map(|ctrl| match ctrl {
                    Control::AutoNumber(number) => Some(number.number_type),
                    _ => None,
                })
                .collect();
            (para.style_id, kinds)
        })
        .collect()
}

#[test]
fn swapping_notes_keeps_content_and_place_and_renumbers() {
    let mut doc = notes_doc();
    let styles = note_paragraph_kinds(&doc);
    let (footnote_style, endnote_style) = (styles[0].0, styles[2].0);
    assert_ne!(footnote_style, endnote_style);
    let result = doc.convert_notes_native("swap").unwrap();
    assert!(result.contains("\"toEndnotes\":2") && result.contains("\"toFootnotes\":1"));
    let expected = vec![
        ("en", 1, "가".to_string()),
        ("en", 2, "나".to_string()),
        ("fn", 1, "미".to_string()),
    ];
    assert_eq!(notes(&doc), expected);
    assert_eq!(
        note_paragraph_kinds(&doc),
        vec![
            (endnote_style, vec![AutoNumberType::Endnote]),
            (endnote_style, vec![AutoNumberType::Endnote]),
            (footnote_style, vec![AutoNumberType::Footnote]),
        ]
    );
    for bytes in [doc.export_hwp().unwrap(), doc.export_hwpx().unwrap()] {
        assert_eq!(notes(&HwpDocument::from_bytes(&bytes).unwrap()), expected);
    }
}

#[test]
fn one_way_conversions_number_in_document_order() {
    let mut doc = notes_doc();
    doc.convert_notes_native("toEndnotes").unwrap();
    assert_eq!(
        notes(&doc),
        vec![
            ("en", 1, "가".to_string()),
            ("en", 2, "나".to_string()),
            ("en", 3, "미".to_string()),
        ]
    );
    doc.convert_notes_native("toFootnotes").unwrap();
    assert_eq!(
        notes(&doc),
        vec![
            ("fn", 1, "가".to_string()),
            ("fn", 2, "나".to_string()),
            ("fn", 3, "미".to_string()),
        ]
    );
    assert!(doc.convert_notes_native("sideways").is_err());
}
