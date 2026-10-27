//! 저장 줄 없이 다시 연 문단도 개체 뒤 캐럿과 글자 위치를 저장 줄 경로와 같게 둔다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use serde_json::Value;

const PNG: &[u8] = include_bytes!("../../assets/logo/logo-16.png");

fn runs(core: &DocumentCore) -> Vec<(String, u64, f64)> {
    let layout: Value =
        serde_json::from_str(&core.get_page_text_layout_logical_native(0).unwrap()).unwrap();
    layout["runs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|run| run["paraIdx"] == 0)
        .map(|run| {
            (
                run["text"].as_str().unwrap().to_string(),
                run["charStart"].as_u64().unwrap(),
                run["fontSize"].as_f64().unwrap(),
            )
        })
        .collect()
}

fn document(text: &str) -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    if !text.is_empty() {
        core.insert_text_native(0, 0, 0, text).unwrap();
    }
    let made: Value = serde_json::from_str(
        &core
            .insert_picture_native(
                0,
                0,
                1.min(text.chars().count()),
                &[],
                PNG,
                1200,
                400,
                16,
                16,
                "png",
                "",
                None,
                None,
            )
            .unwrap(),
    )
    .unwrap();
    let control = made["controlIdx"].as_u64().unwrap() as usize;
    core.set_picture_properties_native(0, 0, control, r#"{"treatAsChar":true}"#)
        .unwrap();
    core.apply_char_format_native(0, 0, 0, text.chars().count(), r#"{"fontSize":1000}"#)
        .unwrap();
    core
}

#[test]
fn empty_inline_picture_paragraph_keeps_end_anchor_after_hwpx_without_line_cache() {
    let core = document("");
    let reopened = DocumentCore::from_bytes(&core.export_hwpx_native().unwrap()).unwrap();
    // 편집으로 계산한 줄 캐시는 HWPX에 쓰지 않으므로 다시 연 문단은 저장 줄이 없다.
    assert!(reopened.document().sections[0].paragraphs[0]
        .line_segs
        .is_empty());
    let expected = runs(&core);
    assert_eq!(expected, vec![(String::new(), 1, 13.3)]);
    assert_eq!(runs(&reopened), expected);
    assert_eq!(
        reopened.get_cursor_rect_native(0, 0, 1).unwrap(),
        core.get_cursor_rect_native(0, 0, 1).unwrap()
    );
}

#[test]
fn text_after_inline_picture_keeps_logical_char_start_without_line_cache() {
    let core = document("가나");
    let reopened = DocumentCore::from_bytes(&core.export_hwpx_native().unwrap()).unwrap();
    assert!(reopened.document().sections[0].paragraphs[0]
        .line_segs
        .is_empty());
    let starts = |core: &DocumentCore| {
        runs(core)
            .into_iter()
            .filter(|(text, _, _)| !text.is_empty())
            .map(|(text, start, _)| (text, start))
            .collect::<Vec<_>>()
    };
    // 그림이 논리 한 칸을 차지하므로 뒤 글자는 2에서 시작한다. 저장 줄 경로와 같다.
    assert_eq!(
        starts(&reopened),
        vec![("가".to_string(), 0), ("나".to_string(), 2)]
    );
    assert_eq!(starts(&reopened), starts(&core));
}

#[test]
fn overwide_inline_picture_keeps_the_paragraph_end_on_its_own_row() {
    let mut core = document("");
    let page = &core.document().sections[0].section_def.page_def;
    let width = page.width - page.margin_left - page.margin_right;
    let control = core.document().sections[0].paragraphs[0]
        .controls
        .iter()
        .position(|control| matches!(control, rhwp::model::control::Control::Picture(_)))
        .unwrap();
    // 본문 전폭 그림은 들여쓴 첫 줄의 가용 폭을 넘는다. 끝 표시는 폭을 더하지 않는다.
    core.set_picture_properties_native(
        0,
        0,
        control,
        &format!(r#"{{"width":{width},"height":6000}}"#),
    )
    .unwrap();
    core.apply_para_format_native(0, 0, r#"{"indent":6000}"#)
        .unwrap();
    let reopened = DocumentCore::from_bytes(&core.export_hwpx_native().unwrap()).unwrap();
    assert!(reopened.document().sections[0].paragraphs[0]
        .line_segs
        .is_empty());
    let layout: Value =
        serde_json::from_str(&reopened.get_page_control_layout_native(0).unwrap()).unwrap();
    let pictures = layout["controls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|control| control["type"] == "image")
        .collect::<Vec<_>>();
    assert_eq!(pictures.len(), 1, "그림을 한 번만 그린다");
    let picture = pictures[0];
    let end: Value =
        serde_json::from_str(&reopened.get_cursor_rect_native(0, 0, 1).unwrap()).unwrap();
    assert_eq!(runs(&reopened), vec![(String::new(), 1, 13.3)]);
    assert!(
        (end["x"].as_f64().unwrap()
            - picture["x"].as_f64().unwrap()
            - picture["w"].as_f64().unwrap())
        .abs()
            <= 0.1,
        "끝 캐럿은 새 줄 왼쪽이 아니라 그림 오른쪽에 놓인다: {end}, {picture}"
    );
    assert!(
        end["y"].as_f64().unwrap() + end["height"].as_f64().unwrap()
            <= picture["y"].as_f64().unwrap() + picture["h"].as_f64().unwrap() + 0.1,
        "문단 끝 표시가 그림 아래에 빈 줄을 예약하지 않는다: {end}, {picture}"
    );
}
