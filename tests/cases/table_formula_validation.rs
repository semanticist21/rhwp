//! 자유 입력 계산식은 일부만 계산하지 않고 참조 셀·결과·원문 보존을 함께 검증한다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::{document_core::DocumentCore, model::control::Control, wasm_api::HwpDocument};
use serde_json::{json, Value};

fn fixture() -> DocumentCore {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    let created: Value =
        serde_json::from_str(&doc.create_table_native(0, 0, 0, 3, 4).unwrap()).unwrap();
    let parent = created["paraIdx"].as_u64().unwrap() as usize;
    let control = created["controlIdx"].as_u64().unwrap() as usize;
    for (cell, text) in [
        (0, "10"),
        (1, "20"),
        (3, "보호 결과"),
        (4, "3"),
        (5, "4"),
        (6, "문자"),
        (7, "7"),
        (8, "-2"),
        (9, "1,000"),
        (10, "2.5"),
        (11, "0"),
    ] {
        doc.insert_text_in_cell_native(0, parent, control, cell, 0, 0, text)
            .unwrap();
    }
    doc.apply_char_format_in_cell_native(
        0,
        parent,
        control,
        3,
        0,
        0,
        2,
        r##"{"italic":true,"textColor":"#803090"}"##,
    )
    .unwrap();
    // Native 오류는 JsValue가 아니라 실제 명령의 HwpError 경계에서 검사한다.
    let mut core = DocumentCore::new_empty();
    core.set_document(doc.document().clone());
    core
}

fn state(doc: &DocumentCore) -> Value {
    serde_json::to_value(&doc.document().sections[0].paragraphs).unwrap()
}

fn table_address(doc: &DocumentCore) -> (usize, usize) {
    let tables: Vec<_> = doc.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .flat_map(|(parent, para)| {
            para.controls
                .iter()
                .enumerate()
                .filter_map(move |(control, value)| {
                    matches!(value, Control::Table(_)).then_some((parent, control))
                })
        })
        .collect();
    assert_eq!(tables.len(), 1, "표는 한 번만 존재한다");
    tables[0]
}

fn evaluate(doc: &mut DocumentCore, formula: &str, write: bool) -> Value {
    let (parent, control) = table_address(doc);
    serde_json::from_str(
        &doc.evaluate_table_formula(0, parent, control, 0, 3, formula, write)
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn malformed_input_never_returns_a_partial_result_or_overwrites_the_target() {
    let mut doc = fixture();
    let (parent, control) = table_address(&doc);
    let before = state(&doc);
    for formula in [
        "=1+",
        "=SUM(A1:A2,",
        "=SUM(A1:B2) xyz",
        "=(1+2",
        "=1+2)",
        "=SUM(A1:)",
        "=SUM(,A1)",
        "=SUM(A1,)",
        "=SUM(1,,2)",
        "=SUM 1",
        "=1..2+3",
        "=1.2.3",
        "=1$+2",
        "=1;",
        "=1 2",
        "=",
        "=()",
        "=ABS(1,2)",
        "=MOD(1,2,3)",
        "=IF(1,2,3,4)",
        "=IF(1,A1,UNKNOWN(1))",
    ] {
        for write in [false, true] {
            assert!(
                doc.evaluate_table_formula(0, parent, control, 0, 3, formula, write)
                    .is_err(),
                "불완전한 수식: {formula}, write={write}"
            );
            assert_eq!(state(&doc), before, "오류 후 원문·직접 서식: {formula}");
        }
    }
}

#[test]
fn dry_run_references_cover_all_branches_and_are_deduplicated_in_row_order() {
    let mut doc = fixture();
    let before = state(&doc);
    let formula = "=IF(1,SUM(A1:B2)+C3*2,SUM(A3:B3))+A1-A1";
    let dry = evaluate(&mut doc, formula, false);
    assert_eq!(dry["ok"], true);
    assert_eq!(dry["formula"], formula);
    assert_eq!(dry["result"], 42.0);
    let cells = json!([
        {"row":0,"col":0}, {"row":0,"col":1},
        {"row":1,"col":0}, {"row":1,"col":1},
        {"row":2,"col":0}, {"row":2,"col":1}, {"row":2,"col":2},
    ]);
    assert_eq!(dry["cells"], cells);
    assert_eq!(state(&doc), before, "dry-run은 모델을 바꾸지 않는다");
    let written = evaluate(&mut doc, formula, true);
    assert_eq!(written["cells"], cells);
    assert_eq!(written["result"], dry["result"]);
}

#[test]
fn nonfinite_results_fail_before_json_or_target_text_is_written() {
    let mut doc = fixture();
    let (parent, control) = table_address(&doc);
    let before = state(&doc);
    for formula in ["=SQRT(-1)", "=LOG(0)", "=EXP(10000)", "=1/0", "=MAX(right)"] {
        for write in [false, true] {
            assert!(
                doc.evaluate_table_formula(0, parent, control, 0, 3, formula, write)
                    .is_err(),
                "유한하지 않은 계산식: {formula}"
            );
            assert_eq!(state(&doc), before, "오류 후 원문·직접 서식: {formula}");
        }
    }
}

#[test]
fn references_resolve_directions_wildcards_and_existing_skip_semantics() {
    let mut doc = fixture();
    let before = state(&doc);
    for (formula, result, cells) in [
        (
            "=SUM(left)",
            30.0,
            json!([{"row":0,"col":0},{"row":0,"col":1},{"row":0,"col":2}]),
        ),
        (
            "=SUM(below)",
            7.0,
            json!([{"row":1,"col":3},{"row":2,"col":3}]),
        ),
        ("=SUM(above)", 0.0, json!([])),
        ("=SUM(right)", 0.0, json!([])),
        (
            "=SUM(?2:?3)",
            7.0,
            json!([{"row":1,"col":3},{"row":2,"col":3}]),
        ),
        (
            "=SUM(A?:C?)",
            30.0,
            json!([{"row":0,"col":0},{"row":0,"col":1},{"row":0,"col":2}]),
        ),
        (
            "=SUM(C1:A1)",
            30.0,
            json!([{"row":0,"col":0},{"row":0,"col":1},{"row":0,"col":2}]),
        ),
        ("=SUM(A1,A1)+ABS(-2)", 22.0, json!([{"row":0,"col":0}])),
        (
            "=A1+C1+C2",
            10.0,
            json!([{"row":0,"col":0},{"row":0,"col":2},{"row":1,"col":2}]),
        ),
        (
            "=COUNT(A1:D1)",
            2.0,
            json!([{"row":0,"col":0},{"row":0,"col":1},{"row":0,"col":2},{"row":0,"col":3}]),
        ),
        ("=?1", 0.0, json!([{"row":0,"col":3}])),
    ] {
        let value = evaluate(&mut doc, formula, false);
        assert_eq!(value["result"], result, "{formula}");
        assert_eq!(value["cells"], cells, "{formula}");
        assert_eq!(state(&doc), before);
    }
}

#[test]
fn existing_functions_keep_valid_arguments_arithmetic_and_lazy_if() {
    let mut doc = fixture();
    for (formula, expected) in [
        ("=SUM()", 0.0),
        ("=AVERAGE(A1,B1)", 15.0),
        ("=AVG(A1,B1)", 15.0),
        ("=PRODUCT(A2,B2)", 12.0),
        ("=PRODUCT()", 1.0),
        ("=MIN(A1,B1)", 10.0),
        ("=MAX(A1,B1)", 20.0),
        ("=COUNT(A1,B1)", 2.0),
        ("=ABS(-2)", 2.0),
        ("=SQRT(9)", 3.0),
        ("=EXP(0)", 1.0),
        ("=LOG(1)", 0.0),
        ("=LOG10 (100)", 2.0),
        ("=SIN(0)", 0.0),
        ("=COS(0)", 1.0),
        ("=TAN(0)", 0.0),
        ("=ASIN(0)", 0.0),
        ("=ACOS(1)", 0.0),
        ("=ATAN(0)", 0.0),
        ("=RADIAN(180)", std::f64::consts::PI),
        ("=SIGN(-2)", -1.0),
        ("=INT(-2.5)", -2.0),
        ("=CEILING(2.5)", 3.0),
        ("=FLOOR(2.5)", 2.0),
        ("=ROUND(2.5)", 3.0),
        ("=TRUNC(2.5)", 2.0),
        ("=MOD(10,3)", 1.0),
        ("=IF(1,42,SQRT(-1))", 42.0),
        ("@(.5+1.)*2-(-3)", 6.0),
    ] {
        let value = evaluate(&mut doc, formula, false);
        assert!(
            (value["result"].as_f64().unwrap() - expected).abs() < 1e-9,
            "{formula}: {value}"
        );
    }
}

#[test]
fn bounds_and_resource_limits_fail_before_reading_cells_or_changing_the_model() {
    use rhwp::document_core::table_calc::{evaluate_formula, TableContext};
    use std::cell::Cell;

    let mut doc = fixture();
    let (parent, control) = table_address(&doc);
    let before = state(&doc);
    for formula in [
        "=E1",
        "=A4",
        "=A0",
        "=A4294967295",
        "=A4294967296",
        "=SUM(A1:ZZZ99999)",
        "=IF(1,A1,E1)",
        "=IF(0,E1,A1)",
    ] {
        for write in [false, true] {
            assert!(
                doc.evaluate_table_formula(0, parent, control, 0, 3, formula, write)
                    .is_err(),
                "{formula}"
            );
            assert_eq!(state(&doc), before);
        }
    }
    for (row, col) in [(3, 0), (0, 4), (usize::MAX, usize::MAX)] {
        for write in [false, true] {
            assert!(doc
                .evaluate_table_formula(0, parent, control, row, col, "=1", write)
                .is_err());
            assert_eq!(state(&doc), before);
        }
    }
    // 허용되는 마지막 깊이·토큰 수·바이트 수도 함께 잠가 임의로 검증 범위를 줄이지 않는다.
    let deepest = format!("={}1{}", "(".repeat(128), ")".repeat(128));
    assert_eq!(evaluate(&mut doc, &deepest, false)["result"], 1.0);
    let longest_tokens = format!("=ABS(1{}) \n", "+1".repeat(510));
    assert_eq!(evaluate(&mut doc, &longest_tokens, false)["result"], 511.0);
    let longest_bytes = format!("=1{}", " ".repeat(64 * 1024 - 2));
    assert_eq!(evaluate(&mut doc, &longest_bytes, false)["result"], 1.0);
    for formula in [
        format!("={}1{}", "(".repeat(129), ")".repeat(129)),
        format!("=-ABS(1{})", "+1".repeat(510)),
        format!("{longest_bytes} "),
        format!("={}", "9".repeat(400)),
    ] {
        for write in [false, true] {
            assert!(doc
                .evaluate_table_formula(0, parent, control, 0, 3, &formula, write)
                .is_err());
            assert_eq!(state(&doc), before);
        }
    }
    let reads = Cell::new(0);
    let get_cell = |_, _| {
        reads.set(reads.get() + 1);
        Some(1.0)
    };
    let ctx = TableContext {
        row_count: 100_000,
        col_count: 100_000,
        current_row: 0,
        current_col: 0,
    };
    assert!(evaluate_formula("=SUM(A1:ZZZ99999)", &ctx, &get_cell).is_err());
    let ctx = TableContext {
        row_count: 1,
        col_count: usize::MAX,
        current_row: 0,
        current_col: 0,
    };
    assert!(evaluate_formula("=SUM(right)", &ctx, &get_cell).is_err());
    assert_eq!(reads.get(), 0, "너무 큰 범위는 셀 조회·확장 전에 거부한다");
}

#[test]
fn public_json_snapshot_and_both_saved_formats_keep_values_and_direct_format() {
    let mut core = fixture();
    let (parent, control) = table_address(&core);
    let before = state(&core);
    let snapshot = core.save_snapshot_native();
    let formula = "=SUM(\n A1,\tB1)";
    let mut doc = HwpDocument::create_empty();
    doc.set_document(core.document().clone());
    let positional: Value = serde_json::from_str(
        &doc.evaluate_table_formula(0, parent as u32, control as u32, 0, 3, formula, false)
            .unwrap(),
    )
    .unwrap();
    let options = json!({"sectionIdx":0,"parentParaIdx":parent,"controlIdx":control,
        "targetRow":0,"targetCol":3,"formula":formula,"writeResult":false});
    let named: Value =
        serde_json::from_str(&doc.evaluate_table_formula_ex(&options.to_string()).unwrap())
            .unwrap();
    assert_eq!(named, positional);
    assert_eq!(
        named["formula"], formula,
        "공백·줄바꿈도 올바른 JSON으로 반환한다"
    );
    assert_eq!(named["cells"], json!([{"row":0,"col":0},{"row":0,"col":1}]));
    let written = evaluate(&mut core, formula, true);
    assert_eq!(written, positional);
    let after = core.save_snapshot_native();
    let check = |doc: &DocumentCore| {
        // 저장 시 구역 제어가 추가될 수 있으므로 실제 표 주소로 다시 검사한다.
        let (parent, control) = table_address(doc);
        let Control::Table(table) =
            &doc.document().sections[0].paragraphs[parent].controls[control]
        else {
            unreachable!()
        };
        let target = table
            .cells
            .iter()
            .position(|cell| cell.row == 0 && cell.col == 3)
            .unwrap();
        assert_eq!(table.cells[target].paragraphs[0].text, "30");
        for (row, col, text) in [(0, 0, "10"), (0, 1, "20"), (1, 2, "문자"), (2, 1, "1,000")] {
            let cell = table
                .cells
                .iter()
                .find(|cell| cell.row == row && cell.col == col)
                .unwrap();
            assert_eq!(cell.paragraphs[0].text, text);
        }
        let props: Value = serde_json::from_str(
            &doc.get_cell_char_properties_at_native(0, parent, control, target, 0, 0)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(props["italic"], true);
        assert_eq!(props["textColor"], "#803090");
    };
    check(&core);
    for hwpx in [false, true] {
        let bytes = if hwpx {
            core.export_hwpx_native().unwrap()
        } else {
            core.export_hwp_with_adapter_snapshot().unwrap()
        };
        let reopened = HwpDocument::from_bytes(&bytes).unwrap();
        let mut saved = DocumentCore::new_empty();
        saved.set_document(reopened.document().clone());
        check(&saved);
    }
    core.restore_snapshot_native(snapshot).unwrap();
    assert_eq!(state(&core), before);
    core.restore_snapshot_native(after).unwrap();
    check(&core);
}
