//! HWPX에서 변환한 HWP는 HWPX 재저장 뒤에도 원래 저장 조판 계약을 쓴다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::model::document::{HWP5_ORIGIN_HWPX_MARKER_PATH, HWPX_ORIGIN_STREAM_PATH};

fn hwp(hwpx_lineage: bool) -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.insert_text_native(0, 0, 0, &"가나다라마바사아자차카타파하".repeat(8))
        .unwrap();
    if hwpx_lineage {
        let mut document = core.document().clone();
        // 기존 HWPX→HWP 생산자가 쓰는 CFB 스트림으로 실제 파서 판정을 거친다.
        document
            .extra_streams
            .push((HWPX_ORIGIN_STREAM_PATH.to_owned(), b"1".to_vec()));
        core.set_document(document);
    }
    DocumentCore::from_bytes(&core.export_hwp_native().unwrap()).unwrap()
}

fn assert_contract(core: &DocumentCore, hwpx_lineage: bool) {
    let profile = core.document().layout_profile();
    assert_eq!(core.document().provenance.hwpx_lineage, hwpx_lineage);
    assert_eq!(profile.hwpx_stored_layout(), hwpx_lineage);
    assert_eq!(profile.hwp5_stored_pagination_layout(), !hwpx_lineage);
}

#[test]
fn hwp_layout_contract_survives_repeated_hwpx_and_hwp_saves() {
    for hwpx_lineage in [false, true] {
        let original = hwp(hwpx_lineage);
        assert_contract(&original, hwpx_lineage);
        let mut current = original;
        for _ in 0..2 {
            let reopened =
                DocumentCore::from_bytes(&current.export_hwpx_native().unwrap()).unwrap();
            assert_contract(&reopened, hwpx_lineage);
            // 계보 보존이 기존 문단 UTF-16 축 마커를 없애거나 줄 주소를 바꾸면 안 된다.
            assert_eq!(
                reopened
                    .document()
                    .hwpx_aux_entry(HWP5_ORIGIN_HWPX_MARKER_PATH),
                Some(rhwp::model::document::HWP5_ORIGIN_HWPX_PARAGRAPH_AXIS)
            );
            let before = &current.document().sections[0].paragraphs[0];
            let after = &reopened.document().sections[0].paragraphs[0];
            assert_eq!(after.text, before.text);
            assert_eq!(after.char_offsets, before.char_offsets);
            assert_eq!(after.line_segs, before.line_segs);
            current = reopened;
        }
        let reopened =
            DocumentCore::from_bytes(&current.export_hwp_with_adapter_snapshot().unwrap()).unwrap();
        assert_contract(&reopened, hwpx_lineage);
    }
}

#[test]
fn direct_hwpx_keeps_its_layout_contract_without_conversion_markers() {
    let core = hwp(false);
    let mut current =
        DocumentCore::from_bytes(&rhwp::serializer::serialize_hwpx(core.document()).unwrap())
            .unwrap();
    for _ in 0..2 {
        let document = current.document();
        assert!(!document.provenance.hwpx_lineage);
        assert!(document.layout_profile().hwpx_stored_layout());
        assert!(!document.layout_profile().hwp5_stored_pagination_layout());
        assert!(document
            .hwpx_aux_entry(HWP5_ORIGIN_HWPX_MARKER_PATH)
            .is_none());
        current = DocumentCore::from_bytes(&current.export_hwpx_native().unwrap()).unwrap();
    }
}

#[test]
fn mixed_hwp3_lineage_keeps_the_hwp5_producer_axis() {
    let mut core = hwp(true);
    let mut document = core.document().clone();
    document.provenance.hwp3_lineage = true;
    document.is_hwp3_variant = true;
    core.set_document(document);
    let reopened = DocumentCore::from_bytes(&core.export_hwpx_native().unwrap()).unwrap();
    assert_contract(&reopened, true);
    let profile = reopened.document().layout_profile();
    assert!(profile.hwp3_layout());
    assert!(!profile.hwp3_native_layout());
    assert!(reopened
        .document()
        .hwpx_aux_entry(HWP5_ORIGIN_HWPX_MARKER_PATH)
        .is_some());
}
