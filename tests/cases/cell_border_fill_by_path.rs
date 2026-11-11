//! 관계형. 머리말·꼬리말 표와 중첩 표의 셀 테두리·배경을 셀 경로로 고친다(한컴 12.30은 머리말 표에도
//! 표 스타일·셀 테두리/배경을 쓴다). 기준: 고친 셀과 맞닿은 이웃 변이 본문 표처럼 바뀌고, 셀 고유 속성
//! 조회가 그 값을 돌려주며, 변경 기록으로 되돌리면 이전 테두리·배경이 돌아오고, HWP·HWPX 저장 뒤에도 남는다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::model::table::Table;

type Path = Vec<(usize, usize, usize)>;

fn path_of(json: &str) -> Path {
    let v: serde_json::Value = serde_json::from_str(json).unwrap();
    v["cellPath"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            let n = |k: &str| e[k].as_u64().unwrap() as usize;
            (n("controlIndex"), n("cellIndex"), n("cellParaIndex"))
        })
        .collect()
}

/// 본문 문단에서 셀 경로를 따라 표를 찾는다(마지막 항목이 표).
fn table_at<'a>(core: &'a DocumentCore, para: usize, path: &Path) -> &'a Table {
    let mut p = &core.document().sections[0].paragraphs[para];
    for (i, &(ctrl, cell, cell_para)) in path.iter().enumerate() {
        let c = &p.controls[ctrl];
        if i + 1 == path.len() {
            let Control::Table(t) = c else {
                panic!("표가 아님")
            };
            return t;
        }
        p = match c {
            Control::Table(t) => &t.cells[cell].paragraphs[cell_para],
            Control::Header(_) | Control::Footer(_) => {
                &c.header_footer_paragraphs().unwrap()[cell_para]
            }
            _ => panic!("경로 마디"),
        };
    }
    unreachable!()
}

fn fill_color(core: &DocumentCore, table: &Table, cell: usize) -> Option<u32> {
    let bf = &core.document().doc_info.border_fills[table.cells[cell].border_fill_id as usize - 1];
    bf.fill.solid.as_ref().map(|s| s.background_color)
}

fn right_border_width(core: &DocumentCore, table: &Table, cell: usize) -> u8 {
    core.document().doc_info.border_fills[table.cells[cell].border_fill_id as usize - 1].borders[1]
        .width
}

fn left_border_width(core: &DocumentCore, table: &Table, cell: usize) -> u8 {
    core.document().doc_info.border_fills[table.cells[cell].border_fill_id as usize - 1].borders[0]
        .width
}

const EDIT: &str = r##"{"fillType":"solid","fillColor":"#ff0000","borderRight":{"type":1,"width":5,"color":"#000000"},"borderPaintSides":[false,true,false,false]}"##;

fn check(mut core: DocumentCore, para: usize, path: Path) {
    let path_json = serde_json::to_string(
        &path
            .iter()
            .map(|&(c, ce, cp)| serde_json::json!({"controlIndex":c,"cellIndex":ce,"cellParaIndex":cp}))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let before_own = core
        .get_cell_own_properties_by_cell_path_native(0, para, &path, 0)
        .unwrap();
    let before_ids: Vec<u16> = table_at(&core, para, &path)
        .cells
        .iter()
        .map(|c| c.border_fill_id)
        .collect();
    let response: serde_json::Value = serde_json::from_str(
        &core
            .set_cell_properties_by_cell_path_native(0, para, &path, 0, EDIT)
            .unwrap(),
    )
    .unwrap();
    assert!(response["borderPaintUndoId"].is_null());
    let table = table_at(&core, para, &path);
    // 빨강 0xff0000은 COLORREF(BGR)로 0x0000ff다.
    assert_eq!(
        fill_color(&core, table, 0),
        Some(0x0000ff),
        "{path_json}: 칸 배경"
    );
    assert_eq!(
        right_border_width(&core, table, 0),
        5,
        "{path_json}: 칸 오른쪽 선"
    );
    assert_eq!(
        left_border_width(&core, table, 1),
        5,
        "{path_json}: 이웃 칸 왼쪽 선"
    );
    let own: serde_json::Value = serde_json::from_str(
        &core
            .get_cell_own_properties_by_cell_path_native(0, para, &path, 0)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(own["fillColor"], "#ff0000");

    for (name, bytes) in [
        ("HWP", core.export_hwp_native().unwrap()),
        ("HWPX", core.export_hwpx_native().unwrap()),
    ] {
        let reopened = DocumentCore::from_bytes(&bytes).unwrap();
        let t = table_at(&reopened, para, &path);
        assert_eq!(
            fill_color(&reopened, t, 0),
            Some(0x0000ff),
            "{name}: 칸 배경"
        );
        assert_eq!(
            left_border_width(&reopened, t, 1),
            5,
            "{name}: 이웃 칸 왼쪽 선"
        );
    }

    // 변경 기록대로 되돌린다(앱 undo 경로).
    let cells: Vec<serde_json::Value> = response["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| serde_json::json!({"cellIdx": c["cellIdx"], "id": c["beforeId"]}))
        .collect();
    core.apply_cell_border_fill_ids_by_cell_path_native(
        0,
        para,
        &path,
        &serde_json::json!({"cells": cells}).to_string(),
    )
    .unwrap();
    let after_ids: Vec<u16> = table_at(&core, para, &path)
        .cells
        .iter()
        .map(|c| c.border_fill_id)
        .collect();
    assert_eq!(after_ids, before_ids);
    assert_eq!(
        core.get_cell_own_properties_by_cell_path_native(0, para, &path, 0)
            .unwrap(),
        before_own
    );
    assert!(core
        .apply_cell_border_fill_ids_by_cell_path_native(
            0,
            para,
            &path,
            r#"{"borderPaintUndoIds":[1]}"#
        )
        .is_err());
}

#[test]
fn header_table_cell_border_fill_by_path() {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    let hf: serde_json::Value =
        serde_json::from_str(&core.create_header_footer_native(0, true, 0).unwrap()).unwrap();
    let (pi, ci) = (
        hf["paraIndex"].as_u64().unwrap() as usize,
        hf["controlIndex"].as_u64().unwrap() as usize,
    );
    let made = core
        .create_table_in_cell_by_path_native(0, pi, &[(ci, 0, 0)], 0, 1, 2, None, None, true)
        .unwrap();
    let path = path_of(&made);
    assert!(matches!(
        core.document().sections[0].paragraphs[pi].controls[path[0].0],
        Control::Header(_)
    ));
    check(core, pi, path);
}

#[test]
fn nested_table_cell_border_fill_by_path() {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.create_table_native(0, 0, 0, 1, 1).unwrap();
    let (para, ctrl) = core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .find_map(|(pi, p)| {
            p.controls
                .iter()
                .position(|c| matches!(c, Control::Table(_)))
                .map(|ci| (pi, ci))
        })
        .unwrap();
    let made = core
        .create_table_in_cell_by_path_native(0, para, &[(ctrl, 0, 0)], 0, 1, 2, None, None, true)
        .unwrap();
    let path = path_of(&made);
    assert_eq!(path.len(), 2);
    check(core, para, path);
}
