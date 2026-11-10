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
