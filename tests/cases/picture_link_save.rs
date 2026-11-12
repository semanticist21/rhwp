//! 연결 그림의 저장: 읽어 둔 연결 파일 바이트는 화면용이라 저장본에 넣지 않는다.
//!
//! 한/글 12.30이 문서에 포함을 끄고 넣은 그림은 HWP에 BinData 연결 레코드(절대·상대 경로)만,
//! HWPX에 `isEmbeded="0"` 항목만 남긴다. 종전 rhwp는 연결 파일을 주입한 뒤 저장하면 HWP에
//! 쓸모없는 `BIN0001` 스트림을, HWPX에 연결과 별개인 내장 그림을 더 넣었다. 또 표 셀·머리말 안
//! 연결 그림은 경로를 받지 못해 앱이 연결 파일을 찾을 수 없었다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::bin_data::{BinData, BinDataBytes, BinDataContent, BinDataType};
use rhwp::model::control::Control;
use rhwp::wasm_api::HwpDocument;

const PNG: &[u8] = include_bytes!("../../assets/logo/logo-32.png");
const LINK: &str = "/Users/someone/그림/연결.png";

/// 그림 하나를 넣고 그 BinData를 한컴처럼 연결 레코드로 바꾼 HWP를 다시 연다.
fn linked_document(in_cell: bool) -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    if in_cell {
        let table: serde_json::Value =
            serde_json::from_str(&doc.create_table_native(0, 0, 0, 1, 1).unwrap()).unwrap();
        let para = table["paraIdx"].as_u64().unwrap() as usize;
        let control = table["controlIdx"].as_u64().unwrap() as usize;
        doc.insert_picture_in_cell_native(
            0,
            para,
            0,
            &[(control, 0, 0)],
            PNG,
            2400,
            2400,
            32,
            32,
            "png",
            "",
            None,
            None,
        )
        .unwrap();
    } else {
        doc.insert_picture_native(0, 0, 0, &[], PNG, 2400, 2400, 32, 32, "png", "", None, None)
            .unwrap();
    }
    let document = doc.document_mut();
    document.doc_info.bin_data_list[0] = BinData {
        data_type: BinDataType::Link,
        abs_path: Some(LINK.to_string()),
        rel_path: Some("연결.png".to_string()),
        extension: Some("png".to_string()),
        ..Default::default()
    };
    document.bin_data_content[0] = BinDataContent {
        id: 0,
        data: BinDataBytes::Loaded(Vec::new()),
        extension: String::new(),
    };
    document.doc_info.raw_stream = None;
    HwpDocument::from_bytes(&doc.export_hwp().unwrap()).unwrap()
}

fn references(doc: &HwpDocument) -> Vec<serde_json::Value> {
    serde_json::from_str(&doc.get_external_image_references()).unwrap()
}

fn has_utf16(bytes: &[u8], text: &str) -> bool {
    let needle: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    bytes.windows(needle.len()).any(|w| w == needle)
}

fn assert_saved_as_link(doc: &HwpDocument) {
    let hwp = doc.export_hwp().unwrap();
    assert!(
        !has_utf16(&hwp, "BIN0001"),
        "연결 그림은 HWP에 BinData 스트림을 남기지 않는다"
    );
    let reopened = HwpDocument::from_bytes(&hwp).unwrap();
    let list = &reopened.document().doc_info.bin_data_list;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].data_type, BinDataType::Link);
    assert_eq!(list[0].abs_path.as_deref(), Some(LINK));

    let hwpx = HwpDocument::from_bytes(&doc.export_hwpx().unwrap()).unwrap();
    assert!(
        hwpx.document()
            .doc_info
            .bin_data_list
            .iter()
            .all(|bd| bd.data_type == BinDataType::Link),
        "HWPX에도 연결만 남고 내장 그림이 생기지 않는다"
    );
    let refs = references(&hwpx);
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0]["originalPath"], LINK);
}

#[test]
fn loaded_link_is_saved_as_link_only() {
    let mut doc = linked_document(false);
    let refs = references(&doc);
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0]["loaded"], false);
    let key = refs[0]["key"].as_str().unwrap().to_string();
    assert_eq!(doc.inject_external_image_by_key(&key, PNG, LINK), 1);
    assert_saved_as_link(&doc);
}

#[test]
fn cell_picture_link_is_listed_and_keeps_its_path() {
    let mut doc = linked_document(true);
    let refs = references(&doc);
    assert_eq!(refs.len(), 1, "표 셀 안 연결 그림도 연결 목록에 든다");
    assert_eq!(refs[0]["originalPath"], LINK);
    let paths: Vec<Option<String>> = doc.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .filter_map(|c| match c {
            Control::Table(t) => Some(t),
            _ => None,
        })
        .flat_map(|t| &t.cells)
        .flat_map(|cell| &cell.paragraphs)
        .flat_map(|p| &p.controls)
        .filter_map(|c| match c {
            Control::Picture(pic) => Some(pic.image_attr.external_path.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(paths, vec![Some(LINK.to_string())]);
    let key = refs[0]["key"].as_str().unwrap().to_string();
    assert_eq!(doc.inject_external_image_by_key(&key, PNG, LINK), 1);
    assert_saved_as_link(&doc);
}
