//! 떠 있는 그림·도형만 있는 빈 문단은 실개체만 그리고 렌더 트리에 빈 줄을 두지 않는다(#1274).
//! 그래도 편집기는 그 문단에 캐럿을 두고, 그 자리를 누르면 그 문단으로 가야 한다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::wasm_api::HwpDocument;
use serde_json::Value;

/// 1쪽 렌더 트리에 이 문단의 TextLine 이 있는지.
fn has_text_line(doc: &HwpDocument, para: u64) -> bool {
    fn walk(node: &Value, para: u64) -> bool {
        (node["type"] == "TextLine" && node["pi"].as_u64() == Some(para))
            || node["children"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|child| walk(child, para))
    }
    walk(
        &serde_json::from_str(&doc.get_page_render_tree(0).unwrap()).unwrap(),
        para,
    )
}

fn caret(core: &DocumentCore, para: usize) -> Value {
    serde_json::from_str(&core.get_cursor_rect_native(0, para, 0).unwrap()).unwrap()
}

#[test]
fn empty_float_host_keeps_its_caret_without_a_render_line() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let before = caret(&doc, 0);
    let inserted: Value = serde_json::from_str(
        &doc.create_shape_control_native(
            0,
            0,
            0,
            7_200,
            3_600,
            1_000,
            1_000,
            false,
            "InFrontOfText",
            "line",
            false,
            false,
            &[],
        )
        .unwrap(),
    )
    .unwrap();
    let para = inserted["paraIdx"].as_u64().unwrap();
    assert_eq!(para, 0, "직선이 빈 첫 문단에 붙어야 한다");
    assert_eq!(doc.document().sections[0].paragraphs[0].text, "");
    assert!(
        !has_text_line(&doc, para),
        "떠 있는 직선만 있는 빈 문단이 렌더 트리에 빈 줄을 남겼다"
    );

    let after = caret(&doc, 0);
    assert_eq!(
        after, before,
        "직선을 넣은 뒤에도 빈 문단 캐럿은 그대로여야 한다"
    );

    let x = after["x"].as_f64().unwrap();
    let y = after["y"].as_f64().unwrap() + after["height"].as_f64().unwrap() / 2.0;
    let hit: Value = serde_json::from_str(&doc.hit_test_native(0, x, y).unwrap()).unwrap();
    assert_eq!(hit["paragraphIndex"], 0);
    assert_eq!(hit["charOffset"], 0);
}
