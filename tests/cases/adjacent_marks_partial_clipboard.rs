//! 맞붙은 부호 사이에서 시작하거나 끝나는 부분 선택의 복사·오려 두기.
//!
//! 한/글 12.30 Mac: `앞[각주 가][각주 나]뒤`에서 두 각주 사이부터 `뒤`까지 복사해 붙이면
//! `[각주 나]뒤`만 들어간다(한컴 저장본). 종전 rhwp는 그 사이 경계를 뒤 개체 모두 뒤로 읽어
//! 각주 없이 `뒤`만 담았고, 지우기는 거절했다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::control::Control;
use rhwp::wasm_api::HwpDocument;

/// 논리 자리 `앞(0) [가](1) [나](2) 뒤(3)` 끝 4.
fn two_notes() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, "앞뒤").unwrap();
    doc.insert_footnote_native(0, 0, 1).unwrap();
    doc.insert_text_in_footnote_native(0, 0, 2, 0, 2, "가")
        .unwrap();
    doc.insert_footnote_native(0, 0, 1).unwrap();
    doc.insert_text_in_footnote_native(0, 0, 3, 0, 2, "나")
        .unwrap();
    doc
}

/// 각주의 글자 자리.
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

/// 본문 글과 각주 차례(번호, 내용).
fn body(doc: &HwpDocument) -> (String, Vec<(u16, String)>) {
    let para = &doc.document().sections[0].paragraphs[0];
    let notes = para
        .controls
        .iter()
        .filter_map(|ctrl| match ctrl {
            Control::Footnote(note) => Some((
                note.number,
                note.paragraphs
                    .iter()
                    .map(|p| p.text.trim().to_string())
                    .collect(),
            )),
            _ => None,
        })
        .collect();
    (para.text.clone(), notes)
}

fn notes(items: &[(u16, &str)]) -> Vec<(u16, String)> {
    items.iter().map(|(n, t)| (*n, t.to_string())).collect()
}

#[test]
fn copy_from_between_marks_takes_only_the_later_mark() {
    let mut doc = two_notes();
    doc.copy_selection_logical_native(0, 0, 2, 0, 4).unwrap();
    // 붙이기 자리는 논리 자리다(문단 끝 4).
    doc.paste_internal_native(0, 0, 4).unwrap();
    assert_eq!(
        body(&doc),
        (
            "앞뒤뒤".to_string(),
            notes(&[(1, "가"), (2, "나"), (3, "나")])
        )
    );
}

#[test]
fn copy_ending_between_marks_takes_only_the_earlier_mark() {
    let mut doc = two_notes();
    doc.copy_selection_logical_native(0, 0, 0, 0, 2).unwrap();
    doc.paste_internal_native(0, 0, 4).unwrap();
    assert_eq!(
        body(&doc),
        (
            "앞뒤앞".to_string(),
            notes(&[(1, "가"), (2, "나"), (3, "가")])
        )
    );
}

#[test]
fn delete_from_between_marks_keeps_the_earlier_mark() {
    let mut doc = two_notes();
    doc.delete_range_logical_native(0, 0, 2, 0, 4).unwrap();
    assert_eq!(body(&doc), ("앞".to_string(), notes(&[(1, "가")])));
    assert_eq!(note_positions(&doc), vec![1]);
    for bytes in [doc.export_hwp().unwrap(), doc.export_hwpx().unwrap()] {
        let reopened = HwpDocument::from_bytes(&bytes).unwrap();
        assert_eq!(body(&reopened), ("앞".to_string(), notes(&[(1, "가")])));
    }
}

#[test]
fn delete_one_of_two_touching_marks() {
    let mut doc = two_notes();
    doc.delete_range_logical_native(0, 0, 1, 0, 2).unwrap();
    assert_eq!(body(&doc), ("앞뒤".to_string(), notes(&[(1, "나")])));
    assert_eq!(note_positions(&doc), vec![1], "남은 각주는 제자리에 있다");
}

#[test]
fn delete_ending_between_marks_keeps_the_later_mark_in_place() {
    let mut doc = two_notes();
    doc.delete_range_logical_native(0, 0, 0, 0, 2).unwrap();
    assert_eq!(body(&doc), ("뒤".to_string(), notes(&[(1, "나")])));
    assert_eq!(note_positions(&doc), vec![0], "남은 각주는 글 앞에 있다");
}
