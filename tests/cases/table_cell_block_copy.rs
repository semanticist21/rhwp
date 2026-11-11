//! 관계형. 셀 블록 복사(`copyTableCells`·`exportTableCellsHtml`)의 계약: 블록만 남긴 새 표가 칸 너비·
//! 병합·셀 배경을 지키고, 원래 표는 그대로이며, 블록 경계에 걸친 병합 셀은 거절한다.
//! 바깥 여백 0.5mm는 한컴 12.30 Mac에서 바깥 여백 1mm 표의 블록을 본문에 붙여 확인했다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::model::table::Table;

fn table(core: &DocumentCore, para: usize) -> &Table {
    match &core.document().sections[0].paragraphs[para].controls[0] {
        Control::Table(table) => table.as_ref(),
        _ => panic!("표가 없습니다"),
    }
}

/// "위" 문단 뒤 4×4 표. 칸마다 "줄칸" 글, (2,1)~(2,2)를 합치고 (1,1)을 칠한다.
fn setup() -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.insert_text_native(0, 0, 0, "위").unwrap();
    core.split_paragraph_native(0, 0, 1, None).unwrap();
    core.create_table_native(0, 1, 0, 4, 4).unwrap();
    for i in 0..16 {
        core.insert_text_in_cell_native(0, 1, 0, i, 0, 0, &format!("{}{}", i / 4, i % 4))
            .unwrap();
    }
    core.set_cell_properties_native(
        0,
        1,
        0,
        5,
        r##"{"fillType":"solid","fillColor":"#ffcc00"}"##,
    )
    .unwrap();
    core.merge_table_cells_native(0, 1, 0, 2, 1, 2, 2).unwrap();
    core
}

#[test]
fn cell_block_copy_pastes_a_table_of_only_the_block() {
    let mut core = setup();
    let before = format!("{:?}", table(&core, 1));
    let widths = table(&core, 1).get_column_widths();

    let copied: serde_json::Value =
        serde_json::from_str(&core.copy_table_cells_native(0, 1, 0, 1, 1, 2, 2).unwrap()).unwrap();
    // 합친 칸은 두 칸의 문단을 모두 지니며 문단 사이는 빈칸으로 잇는다.
    assert_eq!(copied["text"], "11\t12\n21 22");
    assert_eq!(
        format!("{:?}", table(&core, 1)),
        before,
        "원래 표는 그대로다"
    );

    let html = core
        .export_table_cells_html_native(0, 1, 0, 1, 1, 2, 2)
        .unwrap();
    assert!(
        html.contains(">11<") && html.contains(">21<") && !html.contains(">10<"),
        "{html}"
    );

    let last = core.document().sections[0].paragraphs.len() - 1;
    core.paste_internal_native(0, last, 0).unwrap();
    let check = |core: &DocumentCore| {
        let pasted = core.document().sections[0]
            .paragraphs
            .iter()
            .skip(2)
            .flat_map(|p| &p.controls)
            .find_map(|c| match c {
                Control::Table(t) => Some(t),
                _ => None,
            })
            .expect("붙인 표");
        assert_eq!((pasted.row_count, pasted.col_count), (2, 2));
        assert_eq!(pasted.get_column_widths(), widths[1..3].to_vec());
        let texts: Vec<_> = pasted
            .cells
            .iter()
            .map(|c| c.paragraphs[0].text.clone())
            .collect();
        assert_eq!(texts, ["11", "12", "21"]);
        let merged = pasted.cells.iter().find(|c| c.row == 1).unwrap();
        assert_eq!((merged.col, merged.col_span), (0, 2));
        assert_eq!(
            pasted.cells[0].border_fill_id,
            table(core, 1).cells[5].border_fill_id,
            "칠한 칸의 모양을 지킨다"
        );
        assert_ne!(
            pasted.cells[0].border_fill_id,
            pasted.cells[1].border_fill_id
        );
        assert_eq!(
            (
                pasted.outer_margin_left,
                pasted.outer_margin_right,
                pasted.outer_margin_top,
                pasted.outer_margin_bottom
            ),
            (141, 141, 141, 141)
        );
    };
    check(&core);
    for bytes in [
        core.export_hwp_native().unwrap(),
        core.export_hwpx_native().unwrap(),
    ] {
        check(&DocumentCore::from_bytes(&bytes).unwrap());
    }
}

#[test]
fn cell_block_copy_rejects_a_merged_cell_crossing_the_block_edge() {
    let mut core = setup();
    // (2,1)~(2,2) 병합 셀이 (2,0)~(2,1) 블록의 오른쪽 경계에 걸친다.
    assert!(core.copy_table_cells_native(0, 1, 0, 2, 0, 2, 1).is_err());
    assert!(core
        .export_table_cells_html_native(0, 1, 0, 2, 0, 2, 1)
        .is_err());
    assert!(
        core.copy_table_cells_native(0, 1, 0, 0, 0, 4, 0).is_err(),
        "표 밖"
    );
    assert!(!core.has_internal_clipboard_native());
}

/// 4×4 표(칸마다 "줄칸" 글)에서 (0,0)~(1,1) 블록을 복사하고 (0,0)을 칠해 둔다.
fn paste_setup() -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.insert_text_native(0, 0, 0, "위").unwrap();
    core.split_paragraph_native(0, 0, 1, None).unwrap();
    core.create_table_native(0, 1, 0, 4, 4).unwrap();
    for i in 0..16 {
        core.insert_text_in_cell_native(0, 1, 0, i, 0, 0, &format!("{}{}", i / 4, i % 4))
            .unwrap();
    }
    core.set_cell_properties_native(
        0,
        1,
        0,
        0,
        r##"{"fillType":"solid","fillColor":"#ffcc00"}"##,
    )
    .unwrap();
    core.copy_table_cells_native(0, 1, 0, 0, 0, 1, 1).unwrap();
    core
}

/// 줄마다 칸 글을 모은다. 병합으로 가려진 자리는 "-"다.
fn grid(core: &DocumentCore) -> Vec<Vec<String>> {
    let mut table = table(core, 1).clone();
    table.rebuild_grid();
    (0..table.row_count)
        .map(|r| {
            (0..table.col_count)
                .map(|c| match table.cell_at(r, c) {
                    Some(cell) if cell.row == r && cell.col == c => cell
                        .paragraphs
                        .iter()
                        .map(|p| p.text.as_str())
                        .collect::<Vec<_>>()
                        .join(""),
                    _ => "-".to_string(),
                })
                .collect()
        })
        .collect()
}

fn fill_of(core: &DocumentCore, row: u16, col: u16) -> u16 {
    let mut table = table(core, 1).clone();
    table.rebuild_grid();
    table.cell_at(row, col).unwrap().border_fill_id
}

/// 한컴 12.30 셀 붙이기(2×2 블록을 (2,2)에서): 덮어 쓰기는 모양째, 내용만 덮어 쓰기는 글만 바꾸고
/// 표 밖으로 넘치는 부분은 버린다.
#[test]
fn cell_paste_overwrites_contents_and_shapes_clipped_to_the_table() {
    let mut core = paste_setup();
    let painted = fill_of(&core, 0, 0);
    let plain = fill_of(&core, 2, 2);
    assert_ne!(painted, plain);
    core.paste_table_cells_native(0, 1, 0, 2, 2, "overwrite")
        .unwrap();
    assert_eq!(grid(&core)[2], ["20", "21", "00", "01"]);
    assert_eq!(grid(&core)[3], ["30", "31", "10", "11"]);
    assert_eq!(
        fill_of(&core, 2, 2),
        painted,
        "덮어 쓰기는 셀 배경도 옮긴다"
    );

    let mut core = paste_setup();
    core.paste_table_cells_native(0, 1, 0, 2, 2, "content")
        .unwrap();
    assert_eq!(grid(&core)[2], ["20", "21", "00", "01"]);
    assert_eq!(
        fill_of(&core, 2, 2),
        plain,
        "내용만 덮어 쓰기는 셀 모양을 둔다"
    );

    let mut core = paste_setup();
    core.paste_table_cells_native(0, 1, 0, 3, 3, "overwrite")
        .unwrap();
    assert_eq!(grid(&core).len(), 4, "표를 늘리지 않는다");
    assert_eq!(grid(&core)[3], ["30", "31", "32", "00"]);
}

/// 한컴 12.30 끼워 넣기: 왼쪽·오른쪽은 블록 칸 수만큼 칸을 끼우고 블록을 맨 위부터 놓으며, 블록 마지막
/// 줄이 남은 줄을 병합한다. 위·아래는 줄을 끼우고 블록을 맨 왼쪽부터 놓으며 마지막 칸이 남은 칸을 병합한다.
#[test]
fn cell_paste_inserts_block_columns_and_rows_like_hancom() {
    let mut core = paste_setup();
    core.paste_table_cells_native(0, 1, 0, 2, 2, "left")
        .unwrap();
    assert_eq!(
        grid(&core),
        [
            ["00", "01", "00", "01", "02", "03"],
            ["10", "11", "10", "11", "12", "13"],
            ["20", "21", "-", "-", "22", "23"],
            ["30", "31", "-", "-", "32", "33"],
        ]
    );
    assert_eq!(fill_of(&core, 0, 2), fill_of(&core, 0, 0));

    let mut core = paste_setup();
    core.paste_table_cells_native(0, 1, 0, 2, 2, "right")
        .unwrap();
    assert_eq!(grid(&core)[0], ["00", "01", "02", "00", "01", "03"]);
    assert_eq!(grid(&core)[3], ["30", "31", "32", "-", "-", "33"]);

    let mut core = paste_setup();
    core.paste_table_cells_native(0, 1, 0, 2, 2, "up").unwrap();
    let rows = grid(&core);
    assert_eq!(rows.len(), 6);
    assert_eq!(rows[2], ["00", "01", "-", "-"]);
    assert_eq!(rows[3], ["10", "11", "-", "-"]);
    assert_eq!(rows[4], ["20", "21", "22", "23"]);

    let mut core = paste_setup();
    core.paste_table_cells_native(0, 1, 0, 2, 2, "down")
        .unwrap();
    let rows = grid(&core);
    assert_eq!(rows[3], ["00", "01", "-", "-"]);
    assert_eq!(rows[5], ["30", "31", "32", "33"]);

    // HWP/HWPX 저장 뒤에도 같다.
    for bytes in [
        core.export_hwp_native().unwrap(),
        core.export_hwpx_native().unwrap(),
    ] {
        let reopened = DocumentCore::from_bytes(&bytes).unwrap();
        assert_eq!(grid(&reopened), rows);
    }
}

/// 한컴 12.30: 덮어 쓰기는 블록 줄마다 셀을 차례로 대상 줄의 캐럿 칸부터 놓인 셀에 하나씩 덮어 쓴다.
/// 병합한 블록·병합한 대상도 그대로 받는다(대상의 병합은 그대로). 끼워 넣기는 블록의 병합을 지킨다.
#[test]
fn cell_paste_handles_merged_blocks_and_targets_like_hancom() {
    let mut core = paste_setup();
    core.merge_table_cells_native(0, 1, 0, 0, 0, 0, 1).unwrap();
    core.copy_table_cells_native(0, 1, 0, 0, 0, 1, 2).unwrap();
    let mut right = paste_setup();
    right.merge_table_cells_native(0, 1, 0, 0, 0, 0, 1).unwrap();
    right.copy_table_cells_native(0, 1, 0, 0, 0, 1, 2).unwrap();

    core.paste_table_cells_native(0, 1, 0, 2, 2, "overwrite")
        .unwrap();
    assert_eq!(grid(&core)[2], ["20", "21", "0001", "02"]);
    assert_eq!(grid(&core)[3], ["30", "31", "10", "11"]);

    right
        .paste_table_cells_native(0, 1, 0, 2, 2, "right")
        .unwrap();
    assert_eq!(
        grid(&right),
        [
            ["0001", "-", "02", "0001", "-", "02", "03"],
            ["10", "11", "12", "10", "11", "12", "13"],
            ["20", "21", "22", "-", "-", "-", "23"],
            ["30", "31", "32", "-", "-", "-", "33"],
        ]
    );

    // 병합한 대상: 캐럿 줄의 셀을 차례로 덮어 쓰고 병합은 그대로 둔다.
    let mut target = paste_setup();
    target.copy_table_cells_native(0, 1, 0, 2, 2, 3, 3).unwrap();
    target
        .merge_table_cells_native(0, 1, 0, 0, 0, 0, 1)
        .unwrap();
    target
        .paste_table_cells_native(0, 1, 0, 0, 0, "overwrite")
        .unwrap();
    assert_eq!(grid(&target)[0], ["22", "-", "23", "03"]);
    assert_eq!(grid(&target)[1], ["32", "33", "12", "13"]);
}

#[test]
fn cell_paste_refuses_crossing_merges_and_unknown_modes() {
    let mut core = paste_setup();
    // 끼울 자리(칸 2 왼쪽)에 걸친 병합 셀
    core.merge_table_cells_native(0, 1, 0, 2, 1, 2, 2).unwrap();
    let before = format!("{:?}", table(&core, 1));
    assert!(core
        .paste_table_cells_native(0, 1, 0, 0, 2, "left")
        .is_err());
    assert!(core
        .paste_table_cells_native(0, 1, 0, 0, 0, "sideways")
        .is_err());
    assert_eq!(format!("{:?}", table(&core, 1)), before);
}
