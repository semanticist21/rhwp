//! 관계형. 글자 사이에서 끝나는 누름틀 뒤의 개체를 지운다. 기준: 지운 개체만 사라지고 같은
//! 문단의 각주·누름틀은 제자리에 남으며, HWP·HWPX로 저장해 다시 열어도 그대로다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;

fn field_id(json: &str) -> u32 {
    let v: serde_json::Value = serde_json::from_str(json).unwrap();
    v["fieldId"].as_u64().unwrap() as u32
}

fn shape_at(core: &mut DocumentCore, offset: usize) {
    core.create_shape_control_native(
        0,
        0,
        offset,
        2400,
        1200,
        0,
        0,
        true,
        "Square",
        "rectangle",
        false,
        false,
        &[],
    )
    .unwrap();
}

/// (글, 컨트롤 종류, 논리 자리, 누름틀 값)
fn snapshot(core: &DocumentCore) -> (String, Vec<&'static str>, Vec<usize>, Vec<String>) {
    let p = &core.document().sections[0].paragraphs[0];
    let kinds = p
        .controls
        .iter()
        .map(|c| match c {
            Control::Field(_) => "field",
            Control::Footnote(_) => "footnote",
            Control::Shape(_) => "shape",
            Control::SectionDef(_) => "secd",
            Control::ColumnDef(_) => "cold",
            _ => "other",
        })
        .collect();
    let fields: Vec<serde_json::Value> = serde_json::from_str(&core.get_field_list_json()).unwrap();
    let values = fields
        .iter()
        .map(|f| f["value"].as_str().unwrap().to_owned())
        .collect();
    (p.text.clone(), kinds, p.logical_control_positions(), values)
}

fn delete_last_shape_and_check(
    mut core: DocumentCore,
    expected: (String, Vec<&'static str>, Vec<usize>, Vec<String>),
) {
    let ci = core.document().sections[0].paragraphs[0]
        .controls
        .iter()
        .rposition(|c| matches!(c, Control::Shape(_)))
        .unwrap();
    core.delete_shape_control_native(0, 0, ci).unwrap();
    assert_eq!(snapshot(&core), expected, "지운 직후");
    for (name, bytes) in [
        ("HWP", core.export_hwp_native().unwrap()),
        ("HWPX", core.export_hwpx_native().unwrap()),
    ] {
        let reopened = DocumentCore::from_bytes(&bytes).unwrap();
        assert_eq!(snapshot(&reopened), expected, "{name} 다시 열기");
    }
}

/// `앞쪽 [값값]중간 [각주]뒤[도형]쪽`의 도형을 지우면 각주는 `중간 ` 뒤에 남는다.
#[test]
fn deleting_shape_after_mid_text_field_end_keeps_footnote_slot() {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.insert_text_native(0, 0, 0, "앞쪽 중간 뒤쪽").unwrap();
    let id = field_id(
        &core
            .insert_click_here_field_at(0, 0, 3, "안내", "", "", true)
            .unwrap(),
    );
    core.set_field_value_by_id(id, "값값").unwrap();
    core.clear_active_field();
    // 앞쪽 값값중간 뒤쪽(글자 자리): 각주는 `뒤` 앞(8), 도형은 `쪽` 앞(9).
    core.insert_footnote_native(0, 0, 8).unwrap();
    shape_at(&mut core, 9);
    let before = snapshot(&core);
    assert_eq!(before.0, "앞쪽 값값중간 뒤쪽");
    let footnote = before.1.iter().position(|k| *k == "footnote").unwrap();
    let field = before.1.iter().position(|k| *k == "field").unwrap();
    let mut kinds = before.1.clone();
    let mut positions = before.2.clone();
    let shape = kinds.iter().position(|k| *k == "shape").unwrap();
    kinds.remove(shape);
    positions.remove(shape);
    assert_eq!(kinds[footnote], "footnote");
    assert_eq!(kinds[field], "field");
    delete_last_shape_and_check(
        core,
        (before.0.clone(), kinds, positions, vec!["값값".to_owned()]),
    );
}

/// 이어 붙은 두 누름틀 뒤의 도형을 지워도 두 누름틀이 남는다.
#[test]
fn deleting_shape_after_adjacent_fields_keeps_both_fields() {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.insert_text_native(0, 0, 0, "앞뒤").unwrap();
    let first = field_id(
        &core
            .insert_click_here_field_at(0, 0, 1, "하나", "", "", true)
            .unwrap(),
    );
    core.set_field_value_by_id(first, "가").unwrap();
    core.clear_active_field();
    let second = field_id(
        &core
            .insert_click_here_field_at(0, 0, 2, "둘", "", "", true)
            .unwrap(),
    );
    core.set_field_value_by_id(second, "나").unwrap();
    core.clear_active_field();
    shape_at(&mut core, 3);
    let before = snapshot(&core);
    assert_eq!(before.0, "앞가나뒤");
    assert_eq!(before.3, ["가", "나"]);
    let mut kinds = before.1.clone();
    let mut positions = before.2.clone();
    let shape = kinds.iter().position(|k| *k == "shape").unwrap();
    kinds.remove(shape);
    positions.remove(shape);
    delete_last_shape_and_check(core, (before.0.clone(), kinds, positions, before.3.clone()));
}
