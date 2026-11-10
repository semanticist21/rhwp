//! 고정형. 셀 나누기에서 줄 높이를 같게 나누지 않으면 한컴 12.30처럼 앞 줄들은 글 한 줄 높이(안 여백
//! 포함), 마지막 줄이 나머지를 갖는다. 기준: 한컴 12.30 Mac에서 높이 6000 HWPUNIT 셀(10pt 글 한 줄,
//! 위·아래 안 여백 141)을 '줄 높이를 같게 나누기' 없이 세 줄로 나눈 저장본의 셀 높이 1282·1282·3436.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;

fn heights(equal: bool) -> Vec<u32> {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    let table: serde_json::Value =
        serde_json::from_str(&core.create_table_native(0, 0, 0, 1, 2).unwrap()).unwrap();
    let (para, control) = (
        table["paraIdx"].as_u64().unwrap() as usize,
        table["controlIdx"].as_u64().unwrap() as usize,
    );
    core.insert_text_in_cell_native(0, para, control, 0, 0, 0, "나눌 칸")
        .unwrap();
    core.set_cell_properties_native(0, para, control, 0, r#"{"height":6000}"#)
        .unwrap();
    core.split_table_cell_into_native(0, para, control, 0, 0, 3, 1, equal, false)
        .unwrap();
    let Control::Table(table) = &core.document().sections[0].paragraphs[para].controls[control]
    else {
        panic!("표가 없습니다");
    };
    let mut cells: Vec<_> = table.cells.iter().filter(|c| c.col == 0).collect();
    cells.sort_by_key(|c| c.row);
    cells.iter().map(|c| c.height).collect()
}

#[test]
fn unequal_split_gives_leading_rows_one_line_and_the_rest_to_the_last() {
    assert_eq!(heights(false), [1282, 1282, 3436]);
    assert_eq!(heights(true), [2000, 2000, 2000]);
}
