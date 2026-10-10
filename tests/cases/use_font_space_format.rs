//! 공백 폭 옵션은 선택·빈 문단·셀·스타일의 공통 글자 모양 경로로 적용한다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::{control::Control, paragraph::Paragraph, style::CharShape};
use rhwp::wasm_api::HwpDocument;
use serde_json::{json, Value};

const ON: &str = r#"{"useFontSpace":true}"#;
const OFF: &str = r#"{"useFontSpace":false}"#;

fn value(json: &str) -> Value {
    serde_json::from_str(json).unwrap()
}

fn blank(text: &str) -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, text).unwrap();
    let font_id = doc.find_or_create_font_id_native("Batang");
    assert!(font_id >= 0);
    doc.apply_char_format_native(
        0,
        0,
        0,
        text.chars().count(),
        &json!({"fontId":font_id,"fontSize":1500}).to_string(),
    )
    .unwrap();
    doc.apply_para_format_native(0, 0, r#"{"alignment":"left"}"#)
        .unwrap();
    doc
}

fn props(doc: &HwpDocument, offset: usize) -> Value {
    let native = value(&doc.get_char_properties_at_native(0, 0, offset).unwrap());
    assert_eq!(
        native,
        value(&doc.get_char_properties_at(0, 0, offset).unwrap()),
        "WASM 공개 진입점도 native와 같은 속성을 반환한다"
    );
    native
}

fn shapes(doc: &HwpDocument, para: &Paragraph) -> Vec<CharShape> {
    (0..para.text.chars().count())
        .map(|offset| {
            doc.document().doc_info.char_shapes[para.char_shape_id_at(offset).unwrap() as usize]
                .clone()
        })
        .collect()
}

fn body_shapes(doc: &HwpDocument) -> Vec<CharShape> {
    shapes(doc, &doc.document().sections[0].paragraphs[0])
}

fn caret(doc: &HwpDocument, offset: usize) -> Value {
    value(&doc.get_cursor_rect_native(0, 0, offset).unwrap())
}

fn space_width(doc: &HwpDocument) -> f64 {
    let before = caret(doc, 1);
    let after = caret(doc, 2);
    assert_eq!(before["pageIndex"], after["pageIndex"]);
    assert_eq!(before["y"], after["y"]);
    after["x"].as_f64().unwrap() - before["x"].as_f64().unwrap()
}

fn reopen(doc: &HwpDocument, hwpx: bool) -> HwpDocument {
    let bytes = if hwpx {
        doc.export_hwpx_native().unwrap()
    } else {
        doc.export_hwp_native().unwrap()
    };
    HwpDocument::from_bytes(&bytes).unwrap()
}

fn assert_space(doc: &HwpDocument, enabled: bool) {
    // Batang의 공개 메트릭은 U+0020=341/1024em이다. 15pt=20px이며,
    // 옵션이 꺼지면 반각 10px다. API 좌표는 소수 한 자리까지 방출한다.
    let expected = if enabled { 20.0 * 341.0 / 1024.0 } else { 10.0 };
    assert!(
        (space_width(doc) - expected).abs() < 0.15,
        "공백 캐럿 전진이 영문 슬롯 폭과 다르다"
    );
    let selection = value(&doc.get_selection_rects(0, 0, 1, 0, 2).unwrap());
    let rects = selection.as_array().unwrap();
    assert_eq!(rects.len(), 1);
    assert!((rects[0]["width"].as_f64().unwrap() - expected).abs() < 0.15);
}

#[test]
fn selected_font_space_preserves_mixed_shapes_noop_history_and_both_formats() {
    let mut doc = blank("가 나 바");
    doc.apply_char_format_native(0, 0, 2, 3, r##"{"textColor":"#803090"}"##)
        .unwrap();
    let before = body_shapes(&doc);
    let initial = doc.save_snapshot_native();
    let initial_end = caret(&doc, 5);
    assert_eq!(props(&doc, 1)["useFontSpace"], false);
    assert_space(&doc, false);

    doc.apply_char_format(0, 0, 1, 3, ON).unwrap();
    let after = body_shapes(&doc);
    for (offset, (original, actual)) in before.iter().zip(&after).enumerate() {
        let mut expected = original.clone();
        if (1..3).contains(&offset) {
            expected.use_font_space = true;
            expected.raw_data = None;
        }
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
        assert_eq!(
            props(&doc, offset)["useFontSpace"],
            (1..3).contains(&offset)
        );
    }
    assert_space(&doc, true);
    assert!(caret(&doc, 5)["x"].as_f64().unwrap() < initial_end["x"].as_f64().unwrap());
    let applied = doc.save_snapshot_native();
    let count = doc.document().doc_info.char_shapes.len();
    let applied_end = caret(&doc, 5);
    let applied_lines =
        serde_json::to_value(&doc.document().sections[0].paragraphs[0].line_segs).unwrap();
    doc.apply_char_format_native(0, 0, 1, 3, ON).unwrap();
    assert_eq!(
        doc.document().doc_info.char_shapes.len(),
        count,
        "동형 적용은 모양을 추가하지 않는다"
    );
    assert_eq!(caret(&doc, 5), applied_end);
    assert_eq!(
        serde_json::to_value(&doc.document().sections[0].paragraphs[0].line_segs).unwrap(),
        applied_lines
    );

    for hwpx in [false, true] {
        let saved = reopen(&doc, hwpx);
        assert_eq!(saved.document().sections[0].paragraphs[0].text, "가 나 바");
        for offset in 0..5 {
            assert_eq!(
                props(&saved, offset)["useFontSpace"],
                (1..3).contains(&offset)
            );
        }
        assert_eq!(props(&saved, 2)["textColor"], "#803090");
        assert_space(&saved, true);
    }
    doc.restore_snapshot_native(initial).unwrap();
    assert_eq!(body_shapes(&doc), before);
    assert_eq!(caret(&doc, 5), initial_end);
    assert_space(&doc, false);
    doc.restore_snapshot_native(applied).unwrap();
    assert_eq!(body_shapes(&doc), after);
    assert_eq!(caret(&doc, 5), applied_end);
    doc.apply_char_format_native(0, 0, 1, 3, OFF).unwrap();
    assert_space(&doc, false);
    for hwpx in [false, true] {
        assert_eq!(props(&reopen(&doc, hwpx), 1)["useFontSpace"], false);
    }
}

#[test]
fn font_space_reflows_words_using_the_same_caret_width() {
    let mut doc = blank("가 가 가 가");
    let mut model = doc.document().clone();
    let page = &mut model.sections[0].section_def.page_def;
    page.width = 9300;
    page.margin_left = 750;
    page.margin_right = 750;
    doc.set_document(model);
    // 원래 본문 폭의 저장 줄을 기존 크기 API로 다시 짜서 대조군부터 유효하게 둔다.
    doc.apply_char_format_native(0, 0, 0, 7, r#"{"fontSize":1500}"#)
        .unwrap();
    doc.apply_char_format_native(0, 0, 0, 7, OFF).unwrap();
    assert!(doc.document().sections[0].paragraphs[0].line_segs.len() > 1);
    doc.apply_char_format_native(0, 0, 0, 7, ON).unwrap();
    assert_eq!(
        doc.document().sections[0].paragraphs[0].line_segs.len(),
        1,
        "104px 본문에 전각 네 글자와 341/1024em 공백 세 개는 함께 들어간다"
    );
    assert_space(&doc, true);
    doc.apply_char_format_native(0, 0, 0, 7, OFF).unwrap();
    assert!(doc.document().sections[0].paragraphs[0].line_segs.len() > 1);
    assert_space(&doc, false);
}

#[test]
fn empty_body_and_cell_inherit_font_space_and_path_getter_preserves_neighbors() {
    let mut body = blank("");
    body.apply_char_format_native(0, 0, 0, 0, ON).unwrap();
    assert_eq!(props(&body, 0)["useFontSpace"], true);
    body.insert_text_native(0, 0, 0, "가 나").unwrap();
    assert_space(&body, true);
    for hwpx in [false, true] {
        assert_eq!(props(&reopen(&body, hwpx), 1)["useFontSpace"], true);
    }

    let mut doc = blank("본문 보존");
    let created = value(&doc.create_table_native(0, 0, 0, 1, 2).unwrap());
    let parent = created["paraIdx"].as_u64().unwrap() as usize;
    let control = created["controlIdx"].as_u64().unwrap() as usize;
    let path = json!([{"controlIndex":control,"cellIndex":0,"cellParaIndex":0}]).to_string();
    let font_id = doc.find_or_create_font_id_native("Batang");
    doc.apply_char_format_in_cell_native(
        0,
        parent,
        control,
        0,
        0,
        0,
        0,
        &json!({"fontId":font_id,"fontSize":1500}).to_string(),
    )
    .unwrap();
    doc.insert_text_in_cell_native(0, parent, control, 1, 0, 0, "옆 칸 보존")
        .unwrap();
    let sibling = value(
        &doc.get_cell_char_properties_at_native(0, parent, control, 1, 0, 0)
            .unwrap(),
    );
    let initial = doc.save_snapshot_native();
    doc.apply_char_format_in_cell_by_path_api(0, parent as u32, &path, 0, 0, ON)
        .unwrap();
    doc.insert_text_in_cell_native(0, parent, control, 0, 0, 0, "가 나")
        .unwrap();
    assert_eq!(
        value(
            &doc.get_cell_char_properties_at_by_path_api(0, parent as u32, &path, 1)
                .unwrap()
        )["useFontSpace"],
        true
    );
    let cell_rect = |doc: &HwpDocument, offset| {
        value(
            &doc.get_cursor_rect_in_cell_native(0, parent, control, 0, 0, offset)
                .unwrap(),
        )
    };
    assert!(
        (cell_rect(&doc, 2)["x"].as_f64().unwrap()
            - cell_rect(&doc, 1)["x"].as_f64().unwrap()
            - 20.0 * 341.0 / 1024.0)
            .abs()
            < 0.15
    );
    for hwpx in [false, true] {
        let mut saved = reopen(&doc, hwpx);
        assert_eq!(
            value(
                &saved
                    .get_cell_char_properties_at_by_path_api(0, parent as u32, &path, 1)
                    .unwrap()
            )["useFontSpace"],
            true
        );
        let actual = value(
            &saved
                .get_cell_char_properties_at_native(0, parent, control, 1, 0, 0)
                .unwrap(),
        );
        for key in ["useFontSpace", "fontSize", "textColor", "bold", "italic"] {
            assert_eq!(actual[key], sibling[key], "옆 칸 {key} 보존");
        }
        let Control::Table(table) =
            &saved.document().sections[0].paragraphs[parent].controls[control]
        else {
            panic!("표 없음");
        };
        assert_eq!(table.cells[0].paragraphs[0].text, "가 나");
        assert_eq!(table.cells[1].paragraphs[0].text, "옆 칸 보존");
    }
    doc.restore_snapshot_native(initial).unwrap();
    assert_eq!(
        value(
            &doc.get_cell_char_properties_at_by_path_api(0, parent as u32, &path, 0)
                .unwrap()
        )["useFontSpace"],
        false
    );
}

#[test]
fn style_update_changes_font_space_without_changing_direct_format_or_other_style() {
    let mut doc = blank("가 나");
    doc.insert_paragraph_native(0, 1).unwrap();
    doc.insert_text_native(0, 1, 0, "다른 스타일 보존").unwrap();
    let csid = props(&doc, 0)["charShapeId"].as_u64().unwrap();
    let psid = doc.document().sections[0].paragraphs[0].para_shape_id;
    let style = doc.create_style(
        &json!({"name":"공백 폭 스타일","baseCharShapeId":csid,"baseParaShapeId":psid}).to_string(),
    );
    assert!(style > 0);
    doc.apply_style_native(0, 0, style as usize).unwrap();
    doc.apply_char_format_native(0, 0, 2, 3, r##"{"textColor":"#803090"}"##)
        .unwrap();
    let sibling = serde_json::to_value(&doc.document().sections[0].paragraphs[1]).unwrap();
    let initial = doc.save_snapshot_native();
    assert!(doc.update_style_shapes(style as u32, ON, "{}"));
    assert_eq!(props(&doc, 1)["useFontSpace"], true);
    assert_eq!(
        props(&doc, 2)["textColor"],
        "#803090",
        "스타일 편집은 직접 글자색을 보존한다"
    );
    assert_eq!(
        serde_json::to_value(&doc.document().sections[0].paragraphs[1]).unwrap(),
        sibling
    );
    assert_space(&doc, true);
    for hwpx in [false, true] {
        let saved = reopen(&doc, hwpx);
        assert_eq!(props(&saved, 1)["useFontSpace"], true);
        assert_eq!(props(&saved, 2)["textColor"], "#803090");
        assert_eq!(
            saved.document().sections[0].paragraphs[0].style_id,
            style as u8
        );
        assert_eq!(
            saved.document().sections[0].paragraphs[1].text,
            "다른 스타일 보존"
        );
        assert_space(&saved, true);
    }
    doc.restore_snapshot_native(initial).unwrap();
    assert_eq!(props(&doc, 1)["useFontSpace"], false);
    assert_space(&doc, false);
}
