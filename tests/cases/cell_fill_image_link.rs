//! 셀 배경 그림의 '문서에 포함' 끄기·켜기(한컴 12.30 개체 속성 > 배경 > 그림).
//!
//! 끄면 배경 그림 BinData를 파일 연결 레코드로 바꿔 HWP에는 연결 경로만, HWPX에는 내장하지 않은
//! 항목만 남긴다. 켜면 읽어 둔 바이트를 다시 문서에 넣는다. 그 그림을 쓰는 채우기는 순번이 그대로다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::bin_data::BinDataType;
use rhwp::wasm_api::HwpDocument;

const PNG: &[u8] = include_bytes!("../../assets/logo/logo-32.png");
const LINK: &str = "/Users/someone/그림/배경.png";

fn has_utf16(bytes: &[u8], text: &str) -> bool {
    let needle: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    bytes.windows(needle.len()).any(|w| w == needle)
}

fn image_filled_cell() -> (HwpDocument, u32, u32, u16) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    let table: serde_json::Value =
        serde_json::from_str(&doc.create_table_native(0, 0, 0, 2, 2).unwrap()).unwrap();
    let para = table["paraIdx"].as_u64().unwrap() as u32;
    let control = table["controlIdx"].as_u64().unwrap() as u32;
    let id = doc.register_image_data(PNG, "png") as u16;
    let json = format!(
        r#"{{"fillType":"image","imageBinDataId":{id},"imageFillMode":"fitToSize","imageEffect":0,"imageBrightness":0,"imageContrast":0}}"#
    );
    doc.set_cell_properties(0, para, control, 0, &json).unwrap();
    (doc, para, control, id)
}

fn cell_image_id(doc: &HwpDocument, para: u32, control: u32) -> u64 {
    let props: serde_json::Value =
        serde_json::from_str(&doc.get_cell_own_properties(0, para, control, 0).unwrap()).unwrap();
    assert_eq!(props["fillType"], "image");
    props["imageBinDataId"].as_u64().unwrap()
}

#[test]
fn cell_fill_picture_links_and_embeds_again() {
    let (mut doc, para, control, id) = image_filled_cell();
    assert_eq!(doc.get_image_data(u32::from(id)).unwrap(), PNG);
    assert_eq!(doc.get_image_data_extension(u32::from(id)).unwrap(), "png");
    let changed: serde_json::Value =
        serde_json::from_str(&doc.set_image_data_link(u32::from(id), LINK).unwrap()).unwrap();
    assert_eq!(changed["changed"], true);
    assert_eq!(cell_image_id(&doc, para, control), u64::from(id));

    let refs: Vec<serde_json::Value> =
        serde_json::from_str(&doc.get_external_image_references()).unwrap();
    assert_eq!(refs.len(), 1, "배경 그림 연결도 연결 목록에 든다");
    assert_eq!(refs[0]["originalPath"], LINK);
    assert_eq!(refs[0]["loaded"], true);

    let hwp = doc.export_hwp().unwrap();
    assert!(
        !has_utf16(&hwp, "BIN0001"),
        "HWP에 그림 스트림을 남기지 않는다"
    );
    let reopened = HwpDocument::from_bytes(&hwp).unwrap();
    let record = &reopened.document().doc_info.bin_data_list[usize::from(id) - 1];
    assert_eq!(record.data_type, BinDataType::Link);
    assert_eq!(record.abs_path.as_deref(), Some(LINK));
    assert_eq!(cell_image_id(&reopened, para, control), u64::from(id));

    let hwpx = HwpDocument::from_bytes(&doc.export_hwpx().unwrap()).unwrap();
    let list = &hwpx.document().doc_info.bin_data_list;
    assert!(
        list.iter().all(|bd| bd.data_type == BinDataType::Link),
        "HWPX에도 연결만 남는다"
    );
    assert_eq!(cell_image_id(&hwpx, para, control), u64::from(id));

    // 다시 문서에 포함한다. 읽어 둔 바이트가 그대로 저장된다.
    let changed: serde_json::Value =
        serde_json::from_str(&doc.set_image_data_link(u32::from(id), "").unwrap()).unwrap();
    assert_eq!(changed["changed"], true);
    let reopened = HwpDocument::from_bytes(&doc.export_hwp().unwrap()).unwrap();
    let record = &reopened.document().doc_info.bin_data_list[usize::from(id) - 1];
    assert_eq!(record.data_type, BinDataType::Embedding);
    assert_eq!(
        &reopened.document().bin_data_content[usize::from(id) - 1]
            .data
            .load_shared()[..],
        PNG
    );
    // 이미 그 상태면 바꾸지 않는다.
    let same: serde_json::Value =
        serde_json::from_str(&doc.set_image_data_link(u32::from(id), "").unwrap()).unwrap();
    assert_eq!(same["changed"], false);
}

#[test]
fn unloaded_link_cannot_be_embedded() {
    let (mut doc, _, _, id) = image_filled_cell();
    doc.set_image_data_link(u32::from(id), LINK).unwrap();
    let reopened_bytes = doc.export_hwp().unwrap();
    let mut reopened = HwpDocument::from_bytes(&reopened_bytes).unwrap();
    assert!(reopened.set_image_data_link_native(id, "").is_err());
}
