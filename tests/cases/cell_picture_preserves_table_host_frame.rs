//! 셀에서 넣는 종이 기준 그림은 같은 호스트 표와 뒤 본문 줄을 끌어올리지 않는다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use serde_json::Value;

fn json(value: &str) -> Value {
    serde_json::from_str(value).unwrap()
}

fn saved(core: &DocumentCore) -> [Vec<u8>; 2] {
    [
        core.export_hwp_native().unwrap(),
        core.export_hwpx_native().unwrap(),
    ]
}

fn tables(core: &DocumentCore) -> Vec<Value> {
    json(&core.get_page_control_layout_native(0).unwrap())["controls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|control| control["type"] == "table")
        .cloned()
        .collect()
}

#[test]
fn paper_picture_keeps_the_stored_table_outer_frame_and_following_body_line() {
    let mut seed = DocumentCore::new_empty();
    seed.create_blank_document_native().unwrap();
    let created = json(&seed.create_table_native(0, 0, 0, 2, 2).unwrap());
    let paragraph = created["paraIdx"].as_u64().unwrap() as usize;
    let control = created["controlIdx"].as_u64().unwrap() as usize;
    for cell in 0..4 {
        seed.set_cell_properties_native(0, paragraph, control, cell, r#"{"height":7500}"#)
            .unwrap();
    }
    // 편집 중 캐시뿐 아니라 두 저장 형식의 원본 앵커도 같은 흐름을 소유해야 한다.
    for original in saved(&seed) {
        let mut core = DocumentCore::from_bytes(&original).unwrap();
        let text = core.get_page_text_layout_native(0).unwrap();
        let table = tables(&core);
        let baselines: Vec<_> = saved(&core)
            .iter()
            .map(|bytes| {
                let reopened = DocumentCore::from_bytes(bytes).unwrap();
                (
                    reopened.get_page_text_layout_native(0).unwrap(),
                    tables(&reopened),
                )
            })
            .collect();
        core.insert_picture_native(
            0,
            paragraph,
            0,
            &[(control, 3, 0)],
            include_bytes!("../../assets/logo/logo-16.png"),
            9600,
            9600,
            16,
            16,
            "png",
            "셀에서 넣은 그림",
            Some(31500),
            Some(13500),
        )
        .unwrap();
        assert_eq!(core.get_page_text_layout_native(0).unwrap(), text);
        assert_eq!(tables(&core), table);
        for (bytes, (text, table)) in saved(&core).iter().zip(baselines) {
            let reopened = DocumentCore::from_bytes(bytes).unwrap();
            assert_eq!(reopened.get_page_text_layout_native(0).unwrap(), text);
            assert_eq!(tables(&reopened), table);
            let layout = json(&reopened.get_page_control_layout_native(0).unwrap());
            let pictures: Vec<_> = layout["controls"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|control| control["type"] == "image")
                .collect();
            assert_eq!(pictures.len(), 1);
            for (property, expected) in [("x", 420.0), ("y", 180.0), ("w", 128.0), ("h", 128.0)] {
                assert_eq!(pictures[0][property].as_f64().unwrap(), expected);
            }
        }
    }
}
