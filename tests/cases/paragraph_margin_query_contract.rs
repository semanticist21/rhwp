//! 문단 모양 조회와 실제 조판은 물리 HU·글자 단위를 같은 값으로 해석한다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use serde_json::{json, Value};

fn props(core: &DocumentCore, paragraph: usize) -> Value {
    serde_json::from_str(&core.get_para_properties_at_native(0, paragraph).unwrap()).unwrap()
}

fn cursor(core: &DocumentCore, paragraph: usize) -> Value {
    serde_json::from_str(&core.get_cursor_rect_native(0, paragraph, 0).unwrap()).unwrap()
}

#[test]
fn all_five_margin_queries_decode_the_same_physical_and_character_units() {
    for (raw, expected) in [(400, 2.7), (401, 13.3), (-400, -2.7), (-399, -13.3)] {
        let mut core = DocumentCore::new_empty();
        core.create_blank_document_native().unwrap();
        let patch = json!({
            "marginLeft":raw,"marginRight":raw,"indent":raw,
            "spacingBefore":raw,"spacingAfter":raw,
        });
        core.apply_para_format_native(0, 0, &patch.to_string())
            .unwrap();
        for bytes in [
            core.export_hwp_native().unwrap(),
            core.export_hwpx_native().unwrap(),
        ] {
            let saved = DocumentCore::from_bytes(&bytes).unwrap();
            for document in [&core, &saved] {
                let values = props(document, 0);
                for key in [
                    "marginLeft",
                    "marginRight",
                    "indent",
                    "spacingBefore",
                    "spacingAfter",
                ] {
                    assert_eq!(values[key], expected, "{key}, raw={raw}: {values}");
                }
            }
        }
    }
}

#[test]
fn spacing_before_moves_stored_lines_immediately_and_restores_by_shape_id() {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.insert_text_native(0, 0, 0, "앞").unwrap();
    core.insert_paragraph_native(0, 1).unwrap();
    core.insert_text_native(0, 1, 0, "대상").unwrap();
    core.insert_paragraph_native(0, 2).unwrap();
    core.insert_text_native(0, 2, 0, "뒤").unwrap();
    let shape = core.document().sections[0].paragraphs[1].para_shape_id;
    let before = [cursor(&core, 1), cursor(&core, 2)];
    core.apply_para_format_native(0, 1, r#"{"spacingBefore":1500}"#)
        .unwrap();
    let changed = [cursor(&core, 1), cursor(&core, 2)];
    for (old, new) in before.iter().zip(&changed) {
        assert!((new["y"].as_f64().unwrap() - old["y"].as_f64().unwrap() - 10.0).abs() < 0.11);
    }
    for bytes in [
        core.export_hwp_native().unwrap(),
        core.export_hwpx_native().unwrap(),
    ] {
        let saved = DocumentCore::from_bytes(&bytes).unwrap();
        assert_eq!([cursor(&saved, 1), cursor(&saved, 2)], changed);
    }
    core.set_para_shape_id_native(0, 1, shape).unwrap();
    assert_eq!([cursor(&core, 1), cursor(&core, 2)], before);
}

#[test]
fn fractional_css_margins_keep_physical_units_after_both_formats() {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.paste_html_native(0, 0, 0,
        r#"<p style="margin-left:19.7px;margin-right:7.3px;text-indent:-5.1px;margin-top:9.9px;margin-bottom:3.7px">문단</p>"#,
    ).unwrap();
    let expected = [
        ("marginLeft", 14.6),
        ("marginRight", 7.3),
        ("indent", -5.1),
        ("spacingBefore", 9.9),
        ("spacingAfter", 3.7),
    ];
    for bytes in [
        core.export_hwp_native().unwrap(),
        core.export_hwpx_native().unwrap(),
    ] {
        let saved = DocumentCore::from_bytes(&bytes).unwrap();
        for document in [&core, &saved] {
            let values = props(document, 0);
            for (key, expected) in expected {
                assert_eq!(values[key], expected, "{key}: {values}");
            }
        }
    }
}
