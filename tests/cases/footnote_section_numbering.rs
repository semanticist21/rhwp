//! 각주 번호는 미주와 같은 규칙으로 구역마다 각주 모양의 번호 매기기를 따른다(한/글 12.30 Mac).
//!
//! - 현재 구역부터 새로 시작: 각주가 없는 구역이라도 그 구역의 시작 번호에서 다시 센다.
//! - 각주 번호의 새 번호로 시작: 다음 각주 번호를 그 값으로 정한다.
//!
//! 고정 문서는 두 구역 문서를 한/글이 열어 번호를 다시 센 뒤 저장한 것이다. 둘 다 구역 0은
//! 5부터 새로 시작이고 각주가 없으며, 구역 1은 앞 구역에 이어서이고 각주가 둘이다.
//! `restart-5-empty`는 저장 번호 1·2를 한/글이 5·6으로, `new-number-20`은 구역 1 첫머리의
//! 각주 번호 20 새 번호로 시작에 따라 7·8을 20·21로 고쳐 저장했다.
//! `page-restart-then-continue`는 구역 0을 쪽마다 새로 시작으로 바꾸고 두 쪽에 각주를 둘씩 둔
//! 것이다. 한/글은 구역 0을 1·2|1·2, 이어 세는 구역 1을 2·3으로 저장했다. 구역 0의 각주를
//! 1·2|1, 1·2|1|1, 1|1·2로 놓아도 구역 1은 2·3이었다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::control::Control;
use rhwp::model::paragraph::Paragraph;
use rhwp::wasm_api::HwpDocument;
use serde_json::Value;

const DIR: &str = "tests/fixtures/footnote_numbering";

fn open(name: &str) -> HwpDocument {
    HwpDocument::from_bytes(&std::fs::read(format!("{DIR}/{name}")).unwrap()).unwrap()
}

fn collect(paragraphs: &[Paragraph], out: &mut Vec<u16>) {
    for para in paragraphs {
        for ctrl in &para.controls {
            match ctrl {
                Control::Footnote(note) => out.push(note.number),
                Control::Table(table) => {
                    for cell in &table.cells {
                        collect(&cell.paragraphs, out);
                    }
                }
                _ => {}
            }
        }
    }
}

/// 구역마다 각주의 저장 번호.
fn numbers(doc: &HwpDocument) -> Vec<Vec<u16>> {
    doc.document()
        .sections
        .iter()
        .map(|section| {
            let mut out = Vec::new();
            collect(&section.paragraphs, &mut out);
            out
        })
        .collect()
}

fn assert_saved(doc: &HwpDocument, expected: &[Vec<u16>]) {
    assert_eq!(numbers(doc), expected);
    for bytes in [doc.export_hwp().unwrap(), doc.export_hwpx().unwrap()] {
        assert_eq!(numbers(&HwpDocument::from_bytes(&bytes).unwrap()), expected);
    }
}

#[test]
fn inserted_footnote_counts_from_an_empty_restarting_section() {
    let mut doc = open("restart-5-empty.hwpx");
    let inserted: Value =
        serde_json::from_str(&doc.insert_footnote_native(1, 0, 0).unwrap()).unwrap();
    assert_eq!(inserted["footnoteNumber"], 5);
    assert_saved(&doc, &[vec![], vec![5, 6, 7]]);
}

#[test]
fn shape_change_renumbers_the_continuing_section() {
    let mut doc = open("restart-5-empty.hwpx");
    doc.apply_footnote_shape_native(0, r#"{"startNumber":7}"#)
        .unwrap();
    assert_saved(&doc, &[vec![], vec![7, 8]]);
}

#[test]
fn new_number_control_sets_the_next_footnote() {
    let mut doc = open("new-number-20.hwpx");
    let (para, control) = doc.document().sections[1]
        .paragraphs
        .iter()
        .enumerate()
        .find_map(|(pi, para)| {
            para.controls
                .iter()
                .position(|c| matches!(c, Control::Footnote(_)))
                .map(|ci| (pi, ci))
        })
        .unwrap();
    doc.delete_footnote_native(1, para, control).unwrap();
    assert_saved(&doc, &[vec![], vec![20]]);

    // 문단 끝, 남은 각주 뒤에 넣는다.
    let end = doc.document().sections[1].paragraphs[para]
        .text
        .chars()
        .count();
    let inserted: Value =
        serde_json::from_str(&doc.insert_footnote_native(1, para, end).unwrap()).unwrap();
    assert_eq!(inserted["footnoteNumber"], 21);
    assert_saved(&doc, &[vec![], vec![20, 21]]);
}

#[test]
fn section_after_a_page_restart_continues_from_two() {
    let mut doc = open("page-restart-then-continue.hwpx");
    assert_eq!(numbers(&doc), [vec![1, 2, 1, 2], vec![2, 3]]);
    let end = doc.document().sections[1].paragraphs[0]
        .text
        .chars()
        .count();
    doc.insert_footnote_native(1, 0, end).unwrap();
    assert_saved(&doc, &[vec![1, 2, 1, 2], vec![2, 3, 4]]);

    // 쪽마다 새로 시작 구역의 둘째 쪽에 넣으면 그 쪽만 늘고 뒤 구역은 그대로다.
    let end = doc.document().sections[0].paragraphs[1]
        .text
        .chars()
        .count();
    doc.insert_footnote_native(0, 1, end).unwrap();
    assert_saved(&doc, &[vec![1, 2, 1, 2, 3], vec![2, 3, 4]]);
}
