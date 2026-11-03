//! 셀 경로로 중첩 표와 머리말 표의 계산식 필드를 넣고, 다시 계산하고, 지운다.
//!
//! 기대값은 한/글 12.30(Mac)에서 직접 계산한 값이다. 표 칸 안에 만든 표와 머리말 안 표에 각각
//! 1·2를 넣고 쉬운 계산식(가로 합계)을 셋째 칸에 넣자 3이 됐고, 1을 10으로 고치자 12가 됐다.
//! 바깥 표의 계산식은 그대로였다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::model::control::{Control, FieldType};
use rhwp::model::paragraph::Paragraph;
use rhwp::wasm_api::HwpDocument;

/// 같은 이름의 wasm 경계 메서드(JSON 경로)가 아니라 엔진 메서드를 부른다.
fn core(doc: &mut HwpDocument) -> &mut DocumentCore {
    doc
}

type Path = Vec<(usize, usize, usize)>;

/// 칸 하나의 경로. `table` 은 표까지의 경로(마지막 마디가 표)다.
fn at(table: &[(usize, usize, usize)], cell: usize) -> Path {
    let mut path = table.to_vec();
    let last = path.last_mut().unwrap();
    last.1 = cell;
    last.2 = 0;
    path
}

fn text(doc: &HwpDocument, host: usize, path: &[(usize, usize, usize)]) -> String {
    doc.get_text_in_cell_by_path(0, host, path, 0, usize::MAX / 2)
        .unwrap()
}

/// 표 셀 문단의 계산식 필드 명령. 없으면 `None`.
fn formula_command(para: &Paragraph) -> Option<String> {
    para.controls.iter().find_map(|c| match c {
        Control::Field(f) if f.field_type == FieldType::Formula => Some(f.command.clone()),
        _ => None,
    })
}

/// 1×3 표를 만들고 1·2를 넣어 셋째 칸에 가로 합계를 둔다. `create_at` 은 표를 만들 문단 경로다.
fn sum_table(doc: &mut HwpDocument, host: usize, create_at: &[(usize, usize, usize)]) -> Path {
    let json = doc
        .create_table_in_cell_by_path_native(0, host, create_at, 0, 1, 3, None, None, false)
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let table: Path = v["cellPath"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["controlIndex"].as_u64().unwrap() as usize,
                e["cellIndex"].as_u64().unwrap() as usize,
                e["cellParaIndex"].as_u64().unwrap() as usize,
            )
        })
        .collect();
    doc.insert_text_in_cell_by_path(0, host, &at(&table, 0), 0, "1")
        .unwrap();
    doc.insert_text_in_cell_by_path(0, host, &at(&table, 1), 0, "2")
        .unwrap();
    let set: serde_json::Value = serde_json::from_str(
        &core(doc)
            .set_table_cell_formula_by_path(0, host, &at(&table, 2), "=SUM(LEFT)", "%g")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(set["text"], "3");
    assert_eq!(
        set["cells"],
        serde_json::json!([{"row":0,"col":0},{"row":0,"col":1}])
    );
    table
}

/// 첫 칸 `1` 을 `10` 으로 고치고 고친 칸으로 다시 계산한다.
fn edit_first_cell(doc: &mut HwpDocument, host: usize, table: &[(usize, usize, usize)]) {
    doc.insert_text_in_cell_by_path(0, host, &at(table, 0), 1, "0")
        .unwrap();
    core(doc)
        .recalculate_table_formulas_by_path(0, host, table, Some(0), true)
        .unwrap();
}

/// 본문 표(문단 0 컨트롤 2) 칸 안 표의 칸 첫 문단.
fn nested_cell_para<'a>(
    doc: &'a HwpDocument,
    table: &[(usize, usize, usize)],
    cell: usize,
) -> &'a Paragraph {
    let Control::Table(outer) = &doc.document().sections[0].paragraphs[0].controls[table[0].0]
    else {
        panic!("본문 표");
    };
    let host = &outer.cells[table[0].1].paragraphs[table[0].2];
    let Control::Table(inner) = &host.controls[table[1].0] else {
        panic!("칸 안 표");
    };
    &inner.cells[cell].paragraphs[0]
}

fn doc_with_body_table() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    doc.create_table_ex_native(0, 0, 0, 3, 3, true, None, None)
        .unwrap();
    doc
}

#[test]
fn nested_table_formula_recalculates_like_hancom() {
    let mut doc = doc_with_body_table();
    let table = sum_table(&mut doc, 0, &[(2, 4, 0)]);
    assert_eq!(table.len(), 2, "바깥 칸 → 안쪽 표");
    assert_eq!(text(&doc, 0, &at(&table, 2)), "3");
    edit_first_cell(&mut doc, 0, &table);
    assert_eq!(text(&doc, 0, &at(&table, 2)), "12");
    assert_eq!(text(&doc, 0, &[(2, 0, 0)]), "", "바깥 표는 그대로");

    let evaluated: serde_json::Value = serde_json::from_str(
        &doc.evaluate_table_formula_by_path(0, 0, &table, 0, 2, "=SUM(LEFT)")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(evaluated["result"], 12.0);
}

#[test]
fn header_table_formula_recalculates_like_hancom() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    doc.create_header_footer_native(0, true, 0).unwrap();
    let info: serde_json::Value =
        serde_json::from_str(&doc.get_header_footer_native(0, true, 0).unwrap()).unwrap();
    let host = info["paraIndex"].as_u64().unwrap() as usize;
    let hf = info["controlIndex"].as_u64().unwrap() as usize;

    let table = sum_table(&mut doc, host, &[(hf, 0, 0)]);
    assert_eq!(table[0], (hf, 0, 0), "머리말 뿌리");
    edit_first_cell(&mut doc, host, &table);
    assert_eq!(text(&doc, host, &at(&table, 2)), "12");
}

#[test]
fn removed_nested_formula_leaves_plain_text() {
    let mut doc = doc_with_body_table();
    let table = sum_table(&mut doc, 0, &[(2, 4, 0)]);
    let removed: serde_json::Value = serde_json::from_str(
        &core(&mut doc)
            .remove_table_cell_formulas_by_path(0, 0, &at(&table, 2))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(removed["removed"], 1);
    assert_eq!(
        text(&doc, 0, &at(&table, 2)),
        "3",
        "결과는 일반 글로 남는다"
    );
    edit_first_cell(&mut doc, 0, &table);
    assert_eq!(
        text(&doc, 0, &at(&table, 2)),
        "3",
        "지운 계산식은 다시 계산하지 않는다"
    );
}

#[test]
fn nested_table_formula_survives_hwp_and_hwpx_roundtrip() {
    let mut doc = doc_with_body_table();
    let table = sum_table(&mut doc, 0, &[(2, 4, 0)]);
    for (label, bytes) in [
        ("hwp", doc.export_hwp_native().unwrap()),
        ("hwpx", doc.export_hwpx_native().unwrap()),
    ] {
        let mut reloaded =
            HwpDocument::new(&bytes).unwrap_or_else(|_| panic!("{label} 다시 읽기 실패"));
        let command = formula_command(nested_cell_para(&reloaded, &table, 2));
        assert!(
            command
                .as_deref()
                .is_some_and(|c| c.starts_with("=SUM(LEFT)")),
            "{label}: 계산식 필드 {command:?}"
        );
        edit_first_cell(&mut reloaded, 0, &table);
        assert_eq!(text(&reloaded, 0, &at(&table, 2)), "12", "{label}");
    }
}
