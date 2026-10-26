use rhwp::model::control::Control;
use rhwp::model::table::Table;
use rhwp::DocumentCore;

#[derive(Debug, PartialEq)]
struct Geometry {
    rows: u16,
    cols: u16,
    width: u32,
    height: u32,
    cells: Vec<(u16, u16, u16, u16, u32, u32, String)>,
}

fn blank() -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().expect("blank document");
    core
}

fn table(core: &DocumentCore) -> (usize, usize, &Table) {
    core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .find_map(|(paragraph, para)| {
            para.controls
                .iter()
                .enumerate()
                .find_map(|(control, item)| match item {
                    Control::Table(table) => Some((paragraph, control, table.as_ref())),
                    _ => None,
                })
        })
        .expect("table control")
}

fn geometry(core: &DocumentCore) -> Geometry {
    let (_, _, table) = table(core);
    Geometry {
        rows: table.row_count,
        cols: table.col_count,
        width: table.common.width,
        height: table.common.height,
        cells: table
            .cells
            .iter()
            .map(|cell| {
                (
                    cell.row,
                    cell.col,
                    cell.row_span,
                    cell.col_span,
                    cell.width,
                    cell.height,
                    cell.paragraphs
                        .iter()
                        .map(|para| para.text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                )
            })
            .collect(),
    }
}

fn fixture(rows: &[u32], columns: &[u32], ordinary: bool) -> DocumentCore {
    let mut core = blank();
    if ordinary {
        core.create_table_native(0, 0, 0, 2, 2)
            .expect("ordinary table");
    } else {
        core.create_table_ex_native(
            0,
            0,
            0,
            rows.len() as u16,
            columns.len() as u16,
            true,
            Some(columns),
            Some(rows),
        )
        .expect("sized table");
    }
    let (paragraph, control, table) = table(&core);
    let count = table.cells.len();
    for cell in 0..count {
        core.insert_text_in_cell_native(0, paragraph, control, cell, 0, 0, &format!("칸{cell}"))
            .expect("cell text");
    }
    core
}

fn merge(core: &mut DocumentCore, row: u16, col: u16, end_row: u16, end_col: u16) {
    let (paragraph, control, _) = table(core);
    core.merge_table_cells_native(0, paragraph, control, row, col, end_row, end_col)
        .expect("merge cells");
}

// 공개 만들기·병합·속성 변경으로 만든 모델 치수를 HTML CSS에 기록한다. 가져오기 계산은 재사용하지 않는다.
fn exact_html(core: &DocumentCore) -> String {
    let (paragraph, control, table) = table(core);
    let html = core
        .export_control_html_native(0, paragraph, &[], control)
        .expect("export HTML");
    // 이 검사는 Common 메타데이터가 없는 외부 HTML의 CSS 치수와 기존 헤더를 검증한다.
    let mut html = html;
    for attribute in ["data-rhwp-common-v1", "data-rhwp-common-v2"] {
        let prefix = format!(" {attribute}=\"");
        while let Some(start) = html.find(&prefix) {
            let end = start
                + prefix.len()
                + html[start + prefix.len()..]
                    .find('"')
                    .expect("exported Common closing quote")
                + 1;
            html.replace_range(start..end, "");
        }
    }
    let html = html
        .replacen("<table ", "<table data-rhwp-geometry=\"exact\" ", 1)
        .replacen(
            "style=\"",
            &format!(
                "style=\"width:{:.2}pt;height:{:.2}pt;",
                f64::from(table.common.width) / 100.0,
                f64::from(table.common.height) / 100.0,
            ),
            1,
        );
    let mut cells: Vec<_> = table.cells.iter().collect();
    cells.sort_by_key(|cell| (cell.row, cell.col));
    let mut pieces = html.split("<td style=\"");
    let mut result = pieces.next().expect("table HTML").to_string();
    for (index, piece) in pieces.enumerate() {
        let cell = cells[index];
        result.push_str(&format!(
            "<td style=\"width:{:.2}pt;height:{:.2}pt;{piece}",
            f64::from(cell.width) / 100.0,
            f64::from(cell.height) / 100.0,
        ));
    }
    result
}

fn import(html: &str) -> DocumentCore {
    let mut core = blank();
    let result = core.paste_html_native(0, 0, 0, html).expect("paste HTML");
    assert!(serde_json::from_str::<serde_json::Value>(&result).unwrap()["ok"] == true);
    core
}

fn assert_saved_geometry(mut core: DocumentCore, expected: &Geometry) {
    assert_eq!(&geometry(&core), expected);
    let (_, _, table) = table(&core);
    assert_eq!(
        u32::from_le_bytes(table.raw_ctrl_data[12..16].try_into().unwrap()),
        expected.width,
    );
    assert_eq!(
        u32::from_le_bytes(table.raw_ctrl_data[16..20].try_into().unwrap()),
        expected.height,
    );
    let saved = [
        core.export_hwp_with_adapter().expect("save HWP"),
        core.export_hwpx_native().expect("save HWPX"),
    ];
    for bytes in saved {
        let reopened = DocumentCore::from_bytes(&bytes).expect("reopen saved table");
        assert_eq!(&geometry(&reopened), expected);
    }
}

#[test]
fn exact_css_restores_created_table_and_partial_or_complete_merges() {
    for mode in 0..5 {
        let mut source = fixture(&[3003, 4004], &[6001, 9002], false);
        match mode {
            1 => merge(&mut source, 0, 0, 0, 1),
            2 => merge(&mut source, 0, 0, 1, 0),
            3 => merge(&mut source, 0, 0, 1, 1),
            4 => {
                merge(&mut source, 0, 0, 1, 0);
                merge(&mut source, 0, 1, 1, 1);
            }
            _ => {}
        }
        let expected = geometry(&source);
        assert_eq!(
            (expected.rows, expected.cols, expected.height),
            (2, 2, 7007)
        );
        assert_saved_geometry(import(&exact_html(&source)), &expected);
    }
}

#[test]
fn exact_css_preserves_original_height_beyond_cell_sum() {
    let mut source = fixture(&[], &[], true);
    let (paragraph, control, _) = table(&source);
    source
        .set_table_properties_native(0, paragraph, control, r#"{"treatAsChar":true}"#)
        .unwrap();
    assert!(table(&source).2.common.treat_as_char);
    let mut expected = geometry(&source);
    assert_eq!(expected.height, 2564);
    assert!(expected.cells.iter().all(|cell| cell.5 == 282));
    // 기본 표는 대상 문단 가용 폭을 넘는다. 너비 제한과 원래 세로 여유를 구분한다.
    expected.width = 40454;
    for cell in &mut expected.cells {
        cell.4 = 20227;
    }
    assert_saved_geometry(import(&exact_html(&source)), &expected);
}

#[test]
fn exact_css_restores_overlapping_rowspans_without_guessing_original_tracks() {
    let mut source = fixture(&[3003, 4004, 3003, 4004, 3003, 4004], &[6001, 9002], false);
    for (row, end) in [(0, 1), (2, 3), (4, 5)] {
        merge(&mut source, row, 0, end, 0);
    }
    for (row, end) in [(0, 2), (3, 5)] {
        merge(&mut source, row, 1, end, 1);
    }
    let expected = geometry(&source);
    assert_eq!(expected.height, 21021);
    assert_eq!(
        expected.cells.iter().map(|cell| cell.5).collect::<Vec<_>>(),
        [7007, 10010, 7007, 11011, 7007],
    );
    assert_saved_geometry(import(&exact_html(&source)), &expected);
}

#[test]
fn exact_css_preserves_cell_width_set_by_public_api_and_scales_only_for_available_width() {
    let mut source = fixture(&[3003, 4004], &[6001, 9002], false);
    let (paragraph, control, _) = table(&source);
    source
        .set_cell_properties_native(0, paragraph, control, 2, r#"{"width":7001}"#)
        .unwrap();
    source
        .set_cell_properties_native(0, paragraph, control, 3, r#"{"width":8002}"#)
        .unwrap();
    assert_saved_geometry(import(&exact_html(&source)), &geometry(&source));

    let wide = fixture(&[3003, 4004], &[25001, 35002], false);
    let mut expected = geometry(&wide);
    expected.width = 40454;
    for cell in &mut expected.cells {
        cell.4 = if cell.1 == 0 { 16855 } else { 23599 };
    }
    assert_saved_geometry(import(&exact_html(&wide)), &expected);

    let mut wide = fixture(&[3003, 4004], &[25001, 35002], false);
    let (paragraph, control, _) = table(&wide);
    wide.set_cell_properties_native(0, paragraph, control, 2, r#"{"width":26001}"#)
        .unwrap();
    wide.set_cell_properties_native(0, paragraph, control, 3, r#"{"width":34002}"#)
        .unwrap();
    let mut expected = geometry(&wide);
    assert_eq!(expected.width, 61003);
    expected.width = 40454;
    // 선언 폭이 넓은 행별 다른 칸은 비례 축소하고, 공유 열과 같은 칸은 기존 반올림을 쓴다.
    for (cell, width) in expected.cells.iter_mut().zip([16579, 23212, 17242, 22548]) {
        cell.4 = width;
    }
    assert_saved_geometry(import(&exact_html(&wide)), &expected);
}

#[test]
fn ordinary_css_keeps_shared_columns_and_height_as_minimum() {
    let html = r#"<table style="height:5pt"><tr><td style="width:10pt;height:10pt"></td><td style="width:20pt;height:10pt"></td></tr><tr><td style="width:15pt;height:10pt"></td><td style="width:15pt;height:10pt"></td></tr></table>"#;
    let target = import(html);
    let expected = geometry(&target);
    assert_eq!((expected.width, expected.height), (3500, 2000));
    assert_eq!(
        expected.cells.iter().map(|cell| cell.4).collect::<Vec<_>>(),
        [1500, 2000, 1500, 2000]
    );
    assert_saved_geometry(target, &expected);
}

#[test]
fn untrusted_exact_width_falls_back_to_shared_columns_without_losing_exact_height() {
    for html in [
        r#"<table data-rhwp-geometry="exact" style="width:10pt;height:70.07pt"><tr><td rowspan="2" style="width:1000pt;height:70.07pt">앞</td><td style="width:1000pt;height:30.03pt">위</td></tr><tr><td style="width:1000pt;height:40.04pt">아래</td></tr></table>"#,
        r#"<table data-rhwp-geometry="exact" style="width:10pt;height:70.07pt"><tr><td rowspan="2" style="width:8pt;height:70.07pt">앞</td><td style="width:2pt;height:30.03pt">위</td></tr><tr><td style="width:8pt;height:40.04pt">아래</td></tr></table>"#,
    ] {
        let target = import(html);
        let expected = geometry(&target);
        assert_eq!((expected.width, expected.height), (1000, 7007));
        assert_eq!(
            expected.cells.iter().map(|cell| cell.4).collect::<Vec<_>>(),
            [500, 500, 500]
        );
        assert_eq!(
            expected.cells.iter().map(|cell| cell.5).collect::<Vec<_>>(),
            [7007, 3003, 4004]
        );
        assert_eq!(
            expected
                .cells
                .iter()
                .map(|cell| cell.6.as_str())
                .collect::<Vec<_>>(),
            ["앞", "위", "아래"]
        );
        for row in 0..expected.rows {
            let width: u64 = expected
                .cells
                .iter()
                .filter(|cell| cell.0 <= row && row < cell.0 + cell.2)
                .map(|cell| u64::from(cell.4))
                .sum();
            assert!(width <= u64::from(expected.width));
            assert!(width <= 40454);
        }
        assert_saved_geometry(target, &expected);
    }
}

#[test]
fn ordinary_css_uses_minimum_height_for_conflicting_rowspans() {
    let html = r#"<table style="height:1pt"><tr><td rowspan="2" style="height:70pt"></td><td style="height:50pt"></td></tr><tr><td style="height:30pt"></td></tr></table>"#;
    let target = import(html);
    let expected = geometry(&target);
    assert_eq!(expected.height, 8000);
    assert_eq!(
        expected.cells.iter().map(|cell| cell.5).collect::<Vec<_>>(),
        [8000, 5000, 3000]
    );
    assert_saved_geometry(target, &expected);
}

#[test]
fn colspan_skips_an_occupied_cell_in_the_middle_of_its_range() {
    let html =
        r#"<table><tr><td></td><td rowspan="2"></td></tr><tr><td colspan="2"></td></tr></table>"#;
    let target = import(html);
    let expected = geometry(&target);
    assert_eq!((expected.rows, expected.cols), (2, 4));
    assert_eq!(
        expected
            .cells
            .iter()
            .map(|cell| (cell.0, cell.1, cell.2, cell.3))
            .collect::<Vec<_>>(),
        [(0, 0, 1, 1), (0, 1, 2, 1), (1, 2, 1, 2)]
    );
    assert_saved_geometry(target, &expected);
}

#[test]
fn invalid_or_zero_css_dimensions_fall_back_and_oversized_grids_or_heights_fail() {
    for value in ["0", "-1", "NaN", "inf", "1e308"] {
        let html = format!(
            r#"<table data-rhwp-geometry="exact" style="width:{value}pt;height:{value}pt"><tr><td style="width:{value}pt;height:{value}pt"></td></tr></table>"#
        );
        let target = import(&html);
        let shape = geometry(&target);
        assert_eq!(shape.height, 1000, "invalid size {value}");
        assert!(shape.width > 0);
        assert_saved_geometry(target, &shape);
    }
    for html in [
        r#"<table><tr><td rowspan="65535"></td></tr><tr><td rowspan="65535"></td></tr></table>"#,
        r#"<table><tr><td colspan="65535"></td><td></td></tr></table>"#,
        r#"<table><tr><td rowspan="64" colspan="65535"></td></tr></table>"#,
        r#"<table><tr><td style="height:42949672.95pt"></td></tr><tr><td style="height:42949672.95pt"></td></tr></table>"#,
    ] {
        let mut target = blank();
        let before = target.document().sections[0].paragraphs.len();
        assert!(target.paste_html_native(0, 0, 0, html).is_err());
        assert_eq!(target.document().sections[0].paragraphs.len(), before);
        assert!(target.document().sections[0].paragraphs.iter().all(|para| {
            para.controls
                .iter()
                .all(|control| !matches!(control, Control::Table(_)))
        }));
    }
}

fn appearance(core: &DocumentCore) -> serde_json::Value {
    let (paragraph, control, table) = table(core);
    let props: serde_json::Value = serde_json::from_str(
        &core
            .get_table_properties_native(0, paragraph, control)
            .unwrap(),
    )
    .unwrap();
    let mut table_values = serde_json::Map::new();
    for key in [
        "cellSpacing",
        "paddingLeft",
        "paddingRight",
        "paddingTop",
        "paddingBottom",
    ] {
        table_values.insert(key.to_string(), props[key].clone());
    }
    let cells: Vec<_> = (0..table.cells.len())
        .map(|cell| {
            let props: serde_json::Value = serde_json::from_str(
                &core
                    .get_cell_properties_by_cell_path_native(0, paragraph, &[(control, 0, 0)], cell)
                    .unwrap(),
            )
            .unwrap();
            let mut values = serde_json::Map::new();
            for key in [
                "paddingLeft",
                "paddingRight",
                "paddingTop",
                "paddingBottom",
                "applyInnerMargin",
                "verticalAlign",
                "borderLeft",
                "borderRight",
                "borderTop",
                "borderBottom",
                "fillType",
                "fillColor",
            ] {
                values.insert(key.to_string(), props[key].clone());
            }
            serde_json::Value::Object(values)
        })
        .collect();
    serde_json::json!({"table": table_values, "cells": cells})
}

#[test]
fn exact_html_preserves_table_and_cell_padding_flags_spacing_alignment_and_all_border_widths() {
    for spacing in [0, 400] {
        let mut source = fixture(&[3003, 4004], &[6001, 9002], false);
        let (paragraph, control, _) = table(&source);
        source.set_table_properties_native(0, paragraph, control, &serde_json::json!({
            "cellSpacing":spacing,"paddingLeft":1234,"paddingRight":567,"paddingTop":890,"paddingBottom":432
        }).to_string()).unwrap();
        for cell in 0..4 {
            let fill_color = ["#000000", "#ffffff", "#00ff00", "#0000ff"][cell];
            let mut props = serde_json::json!({
                "applyInnerMargin":cell != 1,
                "paddingLeft":if cell == 1 { 30000 } else { 0 },
                "paddingRight":if cell == 1 { -1 } else { 123 },
                "paddingTop":if cell == 1 { 30000 } else { 45 },
                "paddingBottom":if cell == 1 { -123 } else { 0 },
                "verticalAlign":cell % 3,
                "fillType":"solid","fillColor":fill_color
            });
            for (index, (side, kind)) in ["borderLeft", "borderRight", "borderTop", "borderBottom"]
                .iter()
                .zip([1, 2, 3, 8])
                .enumerate()
            {
                props[*side] =
                    serde_json::json!({"type":kind,"width":cell*4+index,"color":"#12abcd"});
            }
            source
                .set_cell_properties_native(0, paragraph, control, cell, &props.to_string())
                .unwrap();
        }
        let expected = appearance(&source);
        assert_eq!(expected["table"]["cellSpacing"], spacing);
        assert_eq!(expected["cells"][0]["borderLeft"]["width"], 0);
        let html = exact_html(&source);
        assert!(html.contains("border-left:0.1mm solid"));
        assert!(html.contains("background-color:#000000"));
        assert!(html.contains("background-color:#ffffff"));
        assert!(html.contains("padding:0.45pt 1.23pt 0.00pt 0.00pt"));
        assert!(html.contains(
            "data-rhwp-cell-padding=\"30000,-1,30000,-123\" data-rhwp-inner-margin=\"0\""
        ));
        let mut target = import(&html);
        assert_eq!(geometry(&target), geometry(&source));
        assert_eq!(appearance(&target), expected);
        for bytes in [
            target.export_hwp_with_adapter().unwrap(),
            target.export_hwpx_native().unwrap(),
        ] {
            assert_eq!(
                appearance(&DocumentCore::from_bytes(&bytes).unwrap()),
                expected
            );
        }
    }
}

#[test]
fn css_padding_and_spacing_keep_zero_clamp_large_values_and_ignore_invalid_values() {
    let html = r#"<table style="padding:0pt;border-spacing:4pt"><tr><td style="padding:0pt">칸</td></tr></table>"#;
    let target = import(html);
    let (_, _, parsed_table) = table(&target);
    assert_eq!(parsed_table.cell_spacing, 400);
    assert_eq!(
        (
            parsed_table.padding.left,
            parsed_table.padding.right,
            parsed_table.padding.top,
            parsed_table.padding.bottom
        ),
        (0, 0, 0, 0)
    );
    assert!(parsed_table.cells[0].apply_inner_margin);
    assert_eq!(parsed_table.cells[0].padding.left, 0);
    assert_eq!(
        parsed_table.cells[0].paragraphs[0].line_segs[0].segment_width,
        parsed_table.cells[0].width as i32
    );

    for invalid in ["-1pt", "NaNpt", "infpt", "1e308pt", "autopt"] {
        let html = format!(
            r#"<table data-rhwp-geometry="exact" data-rhwp-table-padding="1,2,3,9999999" data-rhwp-cell-spacing="9999999" style="padding:{invalid};border-spacing:{invalid}"><tr><td data-rhwp-cell-padding="1,2,3,9999999" data-rhwp-inner-margin="1" style="padding:{invalid}">칸</td></tr></table>"#
        );
        let target = import(&html);
        let (_, _, table) = table(&target);
        assert_eq!(table.cell_spacing, 0, "{invalid}");
        assert_eq!(
            (
                table.padding.left,
                table.padding.right,
                table.padding.top,
                table.padding.bottom
            ),
            (510, 510, 141, 141),
            "{invalid}"
        );
        assert!(!table.cells[0].apply_inner_margin, "{invalid}");
    }
    for large in ["327.68pt", "500px"] {
        let flat = import(&format!(
            r#"<table style="width:1000pt;padding:{large}"><tr><td>칸</td></tr></table>"#
        ));
        let (_, _, parsed) = table(&flat);
        assert_eq!(parsed.padding.left, i16::MAX);
        for target in ["table", "td"] {
            let html = format!(
                r#"<table style="width:200px;{}"><tr><td style="{}"><table><tr><td>안쪽</td></tr></table></td></tr></table>"#,
                if target == "table" {
                    format!("padding:{large}")
                } else {
                    String::new()
                },
                if target == "td" {
                    format!("padding:{large}")
                } else {
                    String::new()
                },
            );
            let error = blank()
                .paste_html_native(0, 0, 0, &html)
                .expect_err("nested table has no room");
            assert!(
                error.to_string().contains("가로 공간이 부족합니다"),
                "{target}: {large}: {error}"
            );
        }
    }
    let parsed = import(
        r#"<table style="padding:1pt 2pt 3pt 4pt;padding-right:0pt"><tr><td>칸</td></tr></table>"#,
    );
    let (_, _, parsed_table) = table(&parsed);
    let padding = parsed_table.padding;
    assert_eq!(
        (padding.left, padding.right, padding.top, padding.bottom),
        (400, 0, 100, 300)
    );
}

#[test]
fn exact_border_metadata_is_bounded_and_unmarked_html_uses_css() {
    for value in [
        "1,16,0;1,0,0;1,0,0;1,0,0",
        "18,0,0;1,0,0;1,0,0;1,0,0",
        "1,0,4294967296;1,0,0;1,0,0;1,0,0",
        "1,0,0;1,0,0;1,0,0",
        "1,0,0;1,0,0;1,0,0;1,0,0;1,0,0",
    ] {
        let parsed = import(&format!(
            r#"<table data-rhwp-geometry="exact"><tr><td data-rhwp-borders="{value}" style="border:1px solid #000000">칸</td></tr></table>"#
        ));
        let props = appearance(&parsed);
        assert_eq!(props["cells"][0]["borderLeft"]["type"], 1);
        assert_eq!(props["cells"][0]["borderLeft"]["width"], 4);
    }
    let cell = r#"<tr><td data-rhwp-borders="0,15,4294967295;1,0,0;1,0,0;1,0,0" style="border:1px solid #000000">칸</td></tr>"#;
    let regular = appearance(&import(&format!("<table>{cell}</table>")));
    assert_eq!(regular["cells"][0]["borderLeft"]["type"], 1);
    assert_eq!(regular["cells"][0]["borderLeft"]["width"], 4);
    let exact = appearance(&import(&format!(
        r#"<table data-rhwp-geometry="exact">{cell}</table>"#
    )));
    assert_eq!(exact["cells"][0]["borderLeft"]["type"], 0);
    assert_eq!(exact["cells"][0]["borderLeft"]["width"], 15);
    for (index, (mm, _)) in rhwp::model::style::BORDER_WIDTHS.iter().enumerate() {
        let parsed = import(&format!(
            r#"<table><tr><td style="border:{:.8}pt solid #000000">칸</td></tr></table>"#,
            mm * 72.0 / 25.4
        ));
        assert_eq!(
            appearance(&parsed)["cells"][0]["borderLeft"]["width"],
            index as u8
        );
    }
}
