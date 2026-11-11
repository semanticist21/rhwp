//! 관계형. 한컴은 글자 스타일을 글자에 따로 적는다(HWP 영역 태그 종류 1, HWPX
//! `<hp:t charStyleIDRef>`). 기준: 글자 범위에 준 글자 스타일이 모양을 바꾼 뒤에도 남고,
//! HWP·HWPX 저장과 다시 열기에서 유지되며, 풀면 사라지고, 스타일 정의를 고치면 그 글자만
//! 새 모양이 된다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::DocumentCore;
use rhwp::wasm_api::HwpDocument;

fn json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap()
}

/// `가나다라마바` 본문과 14pt 글자 스타일 하나.
fn fixture() -> (HwpDocument, u32) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, "가나다라마바").unwrap();
    let id = doc.create_style(r#"{"name":"강조","englishName":"Emphasis","type":1}"#);
    assert!(id >= 0);
    assert!(doc.update_style_shapes(id as u32, r#"{"fontSize":1400}"#, ""));
    (doc, id as u32)
}

fn props(core: &DocumentCore, offset: usize) -> serde_json::Value {
    json(&core.get_char_properties_at_native(0, 0, offset).unwrap())
}

fn styles(core: &DocumentCore) -> Vec<i64> {
    (0..6)
        .map(|i| props(core, i)["charStyleId"].as_i64().unwrap())
        .collect()
}

fn reopen(core: &DocumentCore) -> [(&'static str, DocumentCore); 2] {
    [
        (
            "HWP",
            DocumentCore::from_bytes(&core.export_hwp_native().unwrap()).unwrap(),
        ),
        (
            "HWPX",
            DocumentCore::from_bytes(&core.export_hwpx_native().unwrap()).unwrap(),
        ),
    ]
}

#[test]
fn char_style_survives_restyle_edit_and_save() {
    let (mut doc, id) = fixture();
    let s = id as i64;
    doc.set_char_style_range_by_path(0, 0, &[], 1, 4, Some(id))
        .unwrap();
    assert_eq!(styles(&doc), [-1, s, s, s, -1, -1]);
    assert_eq!(props(&doc, 1)["fontSize"], 1400);
    // 한컴처럼 스타일을 준 뒤 모양을 바꾼 글자도 그 스타일로 남는다.
    doc.apply_char_format_native(0, 0, 1, 2, r#"{"italic":true}"#)
        .unwrap();
    assert_eq!(props(&doc, 1)["italic"], true);
    assert_eq!(styles(&doc), [-1, s, s, s, -1, -1]);
    // 스타일 글자 사이에 친 글자는 그 스타일을 잇는다.
    doc.insert_text_native(0, 0, 2, "X").unwrap();
    assert_eq!(
        (0..7)
            .map(|i| props(&doc, i)["charStyleId"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        [-1, s, s, s, s, -1, -1]
    );
    doc.delete_text_native(0, 0, 2, 1).unwrap();

    for (name, reopened) in reopen(&doc) {
        assert_eq!(styles(&reopened), [-1, s, s, s, -1, -1], "{name}");
        assert_eq!(props(&reopened, 1)["italic"], true, "{name}");
        assert_eq!(props(&reopened, 2)["fontSize"], 1400, "{name}");
        // HWP 영역 태그 종류 1(끝 exclusive)이 한컴 저장본과 같은 꼴이다.
        let para = &reopened.document().sections[0].paragraphs[0];
        let tags: Vec<_> = para
            .range_tags
            .iter()
            .filter_map(|t| t.char_style_id().map(|i| (t.start, t.end, i)))
            .collect();
        assert_eq!(
            tags,
            [(para.char_offsets[1], para.char_offsets[4], id)],
            "{name}"
        );
    }
}

#[test]
fn clearing_char_style_restores_paragraph_style_shape() {
    let (mut doc, id) = fixture();
    let base = props(&doc, 0)["charShapeId"].clone();
    doc.set_char_style_range_by_path(0, 0, &[], 0, 6, Some(id))
        .unwrap();
    // 가운데 둘만 푼다.
    doc.set_char_style_range_by_path(0, 0, &[], 2, 4, None)
        .unwrap();
    let s = id as i64;
    assert_eq!(styles(&doc), [s, s, -1, -1, s, s]);
    assert_eq!(props(&doc, 2)["charShapeId"], base);
    for (name, reopened) in reopen(&doc) {
        assert_eq!(styles(&reopened), [s, s, -1, -1, s, s], "{name}");
    }
    assert!(doc
        .set_char_style_range_by_path(0, 0, &[], 0, 1, Some(0))
        .is_err());
}

#[test]
fn whole_paragraph_char_style_is_recorded_on_text() {
    let (mut doc, id) = fixture();
    doc.apply_style_native(0, 0, id as usize).unwrap();
    assert_eq!(styles(&doc), [id as i64; 6]);
}

/// 119: 글자 스타일 정의를 고치면 그 스타일을 쓴 글자만 바뀌고, 같은 모양 번호의 보통 글과
/// 스타일을 준 뒤 직접 바꾼 글자는 그대로다.
#[test]
fn char_style_definition_change_reaches_styled_text_only() {
    let (mut doc, id) = fixture();
    let styled_shape = props(&doc, 0)["charShapeId"].clone();
    doc.set_char_style_range_by_path(0, 0, &[], 1, 3, Some(id))
        .unwrap();
    let style_shape = props(&doc, 1)["charShapeId"].as_u64().unwrap() as u32;
    // 4번째 글자는 스타일 없이 스타일과 같은 모양 번호만 쓴다.
    doc.set_char_shape_id_native(0, 0, 4, 5, style_shape)
        .unwrap();
    // 2번째 글자는 스타일을 준 뒤 굵게 했다.
    doc.apply_char_format_native(0, 0, 2, 3, r#"{"bold":true}"#)
        .unwrap();
    assert!(doc.update_style_shapes(id, r#"{"fontSize":1800}"#, ""));
    assert_eq!(props(&doc, 1)["fontSize"], 1800);
    assert_eq!(props(&doc, 2)["fontSize"], 1400);
    assert_eq!(props(&doc, 4)["fontSize"], 1400);
    assert_eq!(props(&doc, 0)["charShapeId"], styled_shape);
    for (name, reopened) in reopen(&doc) {
        assert_eq!(props(&reopened, 1)["fontSize"], 1800, "{name}");
        assert_eq!(props(&reopened, 4)["fontSize"], 1400, "{name}");
    }
}
