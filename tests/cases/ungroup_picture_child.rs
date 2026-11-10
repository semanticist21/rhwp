//! 관계형. 그림이 든 묶음을 풀면 그림은 그림 컨트롤로 돌아와야 한다. 한컴 12.30 Mac은 풀린 그림을
//! 따로 고르고 고칠 수 있는 그림 개체로 둔다. 도형 컨트롤에 담으면 쪽 개체 목록이 그 그림의 문단·컨트롤
//! 번호를 주지 못해 고를 수 없다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use serde_json::{json, Value};

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

/// 본문 문단 0의 떠 있는 그림과 사각형을 묶은 문서와, 묶기 전 그림이 그려진 상자.
fn grouped() -> (DocumentCore, Value) {
    let mut doc = DocumentCore::new_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, "앞쪽 가나다 뒤쪽").unwrap();
    let picture = json(
        &doc.insert_picture_native(
            0,
            0,
            2,
            &[],
            include_bytes!("../../assets/logo/logo-16.png"),
            4800,
            4800,
            16,
            16,
            "png",
            "묶을 그림",
            Some(15000),
            Some(15000),
        )
        .unwrap(),
    );
    let picture = picture["controlIdx"].as_u64().unwrap() as usize;
    doc.set_picture_properties_native(
        0,
        0,
        picture,
        &json!({"treatAsChar": false, "textWrap": "InFrontOfText"}).to_string(),
    )
    .unwrap();
    let rect = json(
        &doc.create_shape_control_native(
            0,
            0,
            2,
            4500,
            3000,
            24000,
            15000,
            false,
            "InFrontOfText",
            "rectangle",
            false,
            false,
            &[],
        )
        .unwrap(),
    );
    let rect = rect["controlIdx"].as_u64().unwrap() as usize;
    let pictures = |doc: &DocumentCore| {
        doc.document().sections[0].paragraphs[0]
            .controls
            .iter()
            .filter(|c| matches!(c, Control::Picture(_)))
            .count()
    };
    assert_eq!(pictures(&doc), 1);
    let drawn = picture_boxes(&doc).remove(0);
    doc.group_shapes_native(0, &[(0, picture), (0, rect)])
        .unwrap();
    assert_eq!(pictures(&doc), 0, "묶은 그림은 묶음 안에 든다");
    (doc, drawn)
}

fn picture_boxes(doc: &DocumentCore) -> Vec<Value> {
    json(&doc.get_page_control_layout_native(0).unwrap())["controls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == "image")
        .cloned()
        .collect()
}

#[test]
fn ungrouped_picture_returns_as_a_picture_control_at_the_same_place() {
    let (fresh, drawn) = grouped();
    let reopened = DocumentCore::from_bytes(&fresh.export_hwp_native().unwrap()).unwrap();
    for mut doc in [fresh, reopened] {
        let group = doc.document().sections[0].paragraphs[0]
            .controls
            .iter()
            .position(|c| matches!(c, Control::Shape(_)))
            .unwrap();
        doc.ungroup_shape_native(0, 0, group).unwrap();
        let picture = doc.document().sections[0].paragraphs[0]
            .controls
            .iter()
            .position(|c| matches!(c, Control::Picture(_)))
            .expect("그림 컨트롤로 돌아온다");
        let after = picture_boxes(&doc);
        assert_eq!(after.len(), 1);
        assert_eq!(after[0]["paraIdx"], 0, "{after:?}");
        assert_eq!(after[0]["controlIdx"], picture);
        for key in ["x", "y", "w", "h"] {
            let (a, b) = (
                drawn[key].as_f64().unwrap(),
                after[0][key].as_f64().unwrap(),
            );
            assert!((a - b).abs() < 1.0, "{key}: {a} -> {b}");
        }
        // 풀린 그림은 그림 속성으로 고칠 수 있고 HWP·HWPX 저장 뒤에도 그림이다.
        doc.set_picture_properties_native(0, 0, picture, r#"{"description":"풀린 그림"}"#)
            .unwrap();
        for bytes in [
            doc.export_hwp_native().unwrap(),
            doc.export_hwpx_native().unwrap(),
        ] {
            let reopened = DocumentCore::from_bytes(&bytes).unwrap();
            let boxes = picture_boxes(&reopened);
            assert_eq!(boxes.len(), 1);
            assert!(
                (boxes[0]["x"].as_f64().unwrap() - after[0]["x"].as_f64().unwrap()).abs() < 1.0
            );
            assert!(matches!(
                reopened.document().sections[0].paragraphs[0].controls[picture],
                Control::Picture(_)
            ));
        }
    }
}
