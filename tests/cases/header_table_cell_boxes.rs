//! 관계형. 머리말 표의 칸 상자를 셀 경로(첫 마디가 머리말)로 찾는다. 기준: 머리말이 걸린 쪽의 머리말
//! 영역에 있는 그 표의 네 칸이다. 머리말 표 글줄은 머리말 문단 번호와 머리말 아래 경로를 달아, 본문 쪽
//! 트리에서 경로 그대로 찾으면 찾지 못한다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::wasm_api::HwpDocument;

#[test]
fn header_table_cell_boxes_by_path() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    let hf: serde_json::Value =
        serde_json::from_str(&doc.create_header_footer_native(0, true, 0).unwrap()).unwrap();
    let (pi, ci) = (
        hf["paraIndex"].as_u64().unwrap() as usize,
        hf["controlIndex"].as_u64().unwrap() as usize,
    );
    let made: serde_json::Value = serde_json::from_str(
        &doc.create_table_in_cell_by_path_native(0, pi, &[(ci, 0, 0)], 0, 2, 2, None, None, true)
            .unwrap(),
    )
    .unwrap();
    let path = made["cellPath"].to_string();
    let boxes: serde_json::Value = serde_json::from_str(
        &doc.get_table_cell_bboxes_by_path(0, pi as u32, &path)
            .unwrap_or_else(|_| "[]".into()),
    )
    .unwrap();
    let boxes = boxes.as_array().unwrap();
    assert_eq!(boxes.len(), 4, "{boxes:?}");
    assert!(boxes.iter().all(|b| b["pageIndex"] == 0));
    // 머리말 영역(위 여백 + 머리말 여백) 안이다.
    let page_def = &doc.document().sections[0].section_def.page_def;
    let body_top = (page_def.margin_top + page_def.margin_header) as f64 / 75.0;
    assert!(
        boxes.iter().all(|b| b["y"].as_f64().unwrap() < body_top),
        "{boxes:?}"
    );
}
