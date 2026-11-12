//! 문서 전체 하이퍼링크 문단 주소(`hyperlink_targets_native`).
//!
//! 한/글 12.30 환경 설정 > 기타 > 하이퍼링크 글자 모양의 문서 전체 적용은 본문·표·글상자
//! 밖(머리말·꼬리말·주석·캡션)의 링크도 바꾼다. 종전 rhwp의 링크 주소(`HyperlinkTarget`)는
//! 표 셀·글상자 경로만 받아 그 링크를 찾거나 다시 꾸밀 수 없었다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::hyperlink::HyperlinkTarget;
use rhwp::model::control::Control;
use rhwp::wasm_api::HwpDocument;

const URI: &str = "https://example.com";

fn json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap()
}

/// 본문·머리말·각주에 글과 링크를 하나씩 둔 문서.
fn doc_with_links() -> (HwpDocument, HyperlinkTarget, HyperlinkTarget) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, "본문 링크").unwrap();
    doc.insert_hyperlink_native(&HyperlinkTarget::body(0, 0), 3, 5, URI)
        .unwrap();

    let header = json(&doc.create_header_footer_native(0, true, 0).unwrap());
    doc.insert_text_in_header_footer_native(0, true, 0, 0, 0, "머리말 링크")
        .unwrap();
    let header_target = HyperlinkTarget {
        section: 0,
        para: header["paraIndex"].as_u64().unwrap() as usize,
        cell_path: vec![(header["controlIndex"].as_u64().unwrap() as usize, 0, 0)],
    };
    doc.insert_hyperlink_native(&header_target, 4, 6, URI)
        .unwrap();

    let note = json(&doc.insert_footnote_native(0, 0, 2).unwrap());
    let (para, control) = (
        note["paraIdx"].as_u64().unwrap() as usize,
        note["controlIdx"].as_u64().unwrap() as usize,
    );
    // 각주 첫 문단은 번호 컨트롤을 품어 링크를 넣지 못한다. 둘째 문단에 둔다.
    doc.insert_text_in_footnote_native(0, para, control, 0, 2, "각주 링크")
        .unwrap();
    doc.split_paragraph_in_footnote_native(0, para, control, 0, 2, None)
        .unwrap();
    let note_target = HyperlinkTarget {
        section: 0,
        para,
        cell_path: vec![(control, 0, 1)],
    };
    doc.insert_hyperlink_native(&note_target, 3, 5, URI)
        .unwrap();
    // 각주를 넣으면 같은 문단의 머리말 컨트롤 번호가 뒤로 밀린다. 넣은 뒤 다시 찾는다.
    let controls = &doc.document().sections[0].paragraphs[header_target.para].controls;
    let header_control = controls
        .iter()
        .position(|c| matches!(c, Control::Header(_)))
        .unwrap();
    let note_control = controls
        .iter()
        .position(|c| matches!(c, Control::Footnote(_)))
        .unwrap();
    let header_target = HyperlinkTarget {
        cell_path: vec![(header_control, 0, 0)],
        ..header_target
    };
    let note_target = HyperlinkTarget {
        cell_path: vec![(note_control, 0, 1)],
        ..note_target
    };
    (doc, header_target, note_target)
}

#[test]
fn hyperlink_targets_reach_header_and_notes() {
    let (doc, header, note) = doc_with_links();
    let targets = doc.hyperlink_targets_native();
    assert!(
        targets.contains(&HyperlinkTarget::body(0, 0)),
        "{targets:?}"
    );
    assert!(targets.contains(&header), "{targets:?}");
    assert!(targets.contains(&note), "{targets:?}");
    for target in [&header, &note] {
        let links = doc.hyperlinks_native(target).unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].text, "링크");
    }
}
