//! 관계형. `getControls` 주소로 각주·미주 정보를 읽는다. footnote-01.hwp의 3번 각주는 표 셀 안에 있어
//! 본문 주소만 받는 `getFootnoteInfo`로는 읽을 수 없었다(한컴 12.30 주석 저장하기는 이 각주도 문서
//! 차례로 모은다). 기준: 본문 주석은 두 조회가 같은 값, 셀 안 주석도 글을 주고 번호는 문서 차례다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::DocumentCore;
use std::path::Path;

#[test]
fn note_info_in_list_reads_body_and_cell_notes_in_document_order() {
    let bytes =
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/footnote-01.hwp"))
            .unwrap();
    let core = DocumentCore::from_bytes(&bytes).unwrap();
    let controls: Vec<serde_json::Value> = serde_json::from_str(&core.controls_json()).unwrap();
    let mut numbers = Vec::new();
    let mut cell_notes = 0;
    for control in controls
        .iter()
        .filter(|c| c["ctrlId"] == "fn" || c["ctrlId"] == "en")
    {
        let (list, para, index) = (
            control["list"].as_u64().unwrap() as u32,
            control["para"].as_u64().unwrap() as usize,
            control["controlIndex"].as_u64().unwrap() as usize,
        );
        let raw = core
            .get_note_info_in_list_native(list, para, index)
            .unwrap();
        let info: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(
            info["texts"].as_array().unwrap().iter().any(|t| t != ""),
            "주석 {control}의 글"
        );
        if control["ctrlId"] == "fn" {
            numbers.push(info["number"].as_u64().unwrap());
        }
        if list == 0 {
            // 본문 리스트의 문단 번호는 구역을 이어 센다.
            let mut sec = 0;
            let mut local = para;
            while local >= core.document().sections[sec].paragraphs.len() {
                local -= core.document().sections[sec].paragraphs.len();
                sec += 1;
            }
            assert_eq!(
                raw,
                core.get_footnote_info_native(sec, local, index).unwrap()
            );
        } else {
            cell_notes += 1;
        }
    }
    assert!(cell_notes >= 1, "표 셀 안 각주가 있다");
    assert!(
        numbers.windows(2).all(|w| w[0] < w[1]),
        "문서 차례의 번호 {numbers:?}"
    );
    assert!(core.get_note_info_in_list_native(0, 0, 999).is_err());
}
