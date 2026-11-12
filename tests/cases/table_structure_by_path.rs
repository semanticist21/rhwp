//! 관계형. 칸 안의 표와 머리말 표의 줄·칸 넣기·지우기, 칸 합치기·나누기, 칸 문단 스타일을 셀 경로로
//! 한다(한컴 12.30은 안쪽 표·머리말 표에서도 이 명령을 그 표에 쓴다). 기준: 경로의 표만 바뀌고 바깥 표는
//! 그대로이며, HWP·HWPX 저장 뒤에도 남는다. 평면 API로 부르면 바깥 표가 바뀌던 것(upstream #8138)의 짝이다.
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
fn table_at<'a>(core: &'a DocumentCore, para: usize, path: &[(usize, usize, usize)]) -> &'a Table {
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

fn shape(core: &DocumentCore, para: usize, path: &[(usize, usize, usize)]) -> (u16, u16, usize) {
    let t = table_at(core, para, path);
    (t.row_count, t.col_count, t.cells.len())
}

/// 1×1 본문 표의 칸에 2×2 표를 만든다. (본문 표 문단, 안쪽 표 첫 칸 경로)
fn nested() -> (DocumentCore, usize, Path) {
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
        .create_table_in_cell_by_path_native(0, para, &[(ctrl, 0, 0)], 0, 2, 2, None, None, true)
        .unwrap();
    (core, para, path_of(&made))
}

/// 머리말에 2×2 표를 만든다. (머리말을 가진 본문 문단, 머리말 표 첫 칸 경로)
fn header() -> (DocumentCore, usize, Path) {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    let hf: serde_json::Value =
        serde_json::from_str(&core.create_header_footer_native(0, true, 0).unwrap()).unwrap();
    let (pi, ci) = (
        hf["paraIndex"].as_u64().unwrap() as usize,
        hf["controlIndex"].as_u64().unwrap() as usize,
    );
    let made = core
        .create_table_in_cell_by_path_native(0, pi, &[(ci, 0, 0)], 0, 2, 2, None, None, true)
        .unwrap();
    (core, pi, path_of(&made))
}

fn reopened(core: &DocumentCore) -> Vec<(&'static str, DocumentCore)> {
    [
        ("HWP", core.export_hwp_native().unwrap()),
        ("HWPX", core.export_hwpx_native().unwrap()),
    ]
    .into_iter()
    .map(|(name, bytes)| (name, DocumentCore::from_bytes(&bytes).unwrap()))
    .collect()
}

fn lines_and_cells(core: DocumentCore, para: usize, path: Path) {
    let mut core = core;
    // 본문 표 안의 표면 첫 마디가 본문 표다(머리말 표는 바깥 표가 없다).
    let outer = matches!(
        core.document().sections[0].paragraphs[para].controls[path[0].0],
        Control::Table(_)
    );
    let outer_shape = outer.then(|| shape(&core, para, &path[..1]));
    let r = core
        .edit_table_lines_by_path(0, para, &path, true, false, 1, true)
        .unwrap();
    assert!(r.contains("\"rowCount\":3"), "{r}");
    core.edit_table_lines_by_path(0, para, &path, false, false, 0, false)
        .unwrap();
    assert_eq!(shape(&core, para, &path), (3, 3, 9), "줄·칸 넣기");
    core.edit_table_lines_by_path(0, para, &path, true, true, 0, false)
        .unwrap();
    assert_eq!(shape(&core, para, &path), (2, 3, 6), "줄 지우기");
    core.merge_table_cells_by_path(0, para, &path, 0, 0, 1, 1)
        .unwrap();
    assert_eq!(shape(&core, para, &path), (2, 3, 3), "칸 합치기");
    core.split_table_cells_by_path(0, para, &path, (0, 2, 1, 2), 1, 2, false, false)
        .unwrap();
    assert_eq!(shape(&core, para, &path).2, 5, "칸 범위 나누기");
    if let Some(before) = outer_shape {
        assert_eq!(shape(&core, para, &path[..1]), before, "바깥 표는 그대로");
    }
    let expected = shape(&core, para, &path);
    for (name, doc) in reopened(&core) {
        assert_eq!(shape(&doc, para, &path), expected, "{name} 다시 열기");
        if let Some(before) = outer_shape {
            assert_eq!(shape(&doc, para, &path[..1]), before, "{name} 바깥 표");
        }
    }
}

#[test]
fn nested_table_lines_and_cells_by_path() {
    let (core, para, path) = nested();
    assert_eq!(path.len(), 2);
    lines_and_cells(core, para, path);
}

#[test]
fn header_table_lines_and_cells_by_path() {
    let (core, para, path) = header();
    assert!(matches!(
        core.document().sections[0].paragraphs[para].controls[path[0].0],
        Control::Header(_)
    ));
    lines_and_cells(core, para, path);
}

#[test]
fn nested_table_merge_grows_outer_row() {
    // 안쪽 표 칸에 글을 넣고 합치면 합친 칸에 두 문단이 겹치지 않고 쌓인다.
    let (mut core, para, path) = nested();
    let d = path.len() - 1;
    for cell in 0..2 {
        let mut p = path.clone();
        p[d].1 = cell;
        core.insert_text_in_cell_by_path(0, para, &p, 0, "가나다")
            .unwrap();
    }
    core.merge_table_cells_by_path(0, para, &path, 0, 0, 0, 1)
        .unwrap();
    let t = table_at(&core, para, &path);
    let merged = &t.cells[0];
    assert_eq!(merged.paragraphs.len(), 2);
    let first = &merged.paragraphs[0].line_segs[0];
    let second = &merged.paragraphs[1].line_segs[0];
    assert!(
        second.vertical_pos >= first.vertical_pos + first.line_height,
        "둘째 문단이 첫 문단 아래: {} {}",
        first.vertical_pos,
        second.vertical_pos
    );
}

#[test]
fn cell_style_by_path_reaches_inner_cell_only() {
    for (name, (mut core, para, path)) in [("중첩", nested()), ("머리말", header())] {
        let outer_style =
            |core: &DocumentCore| core.document().sections[0].paragraphs[para].style_id;
        let before = outer_style(&core);
        // 스타일 2는 빈 문서의 '개요 1'이다(문단 스타일).
        let style = core
            .document()
            .doc_info
            .styles
            .iter()
            .position(|s| s.style_type == 0 && s.local_name == "개요 1")
            .unwrap();
        core.apply_cell_style_by_path(0, para, &path, style)
            .unwrap();
        assert_eq!(
            core.get_cell_style_at_by_path(0, para, &path).unwrap(),
            style,
            "{name}"
        );
        assert_eq!(outer_style(&core), before, "{name}: 본문 문단 그대로");
        let mut sibling = path.clone();
        let d = sibling.len() - 1;
        sibling[d].1 = 1;
        assert_eq!(
            core.get_cell_style_at_by_path(0, para, &sibling).unwrap(),
            0,
            "{name}: 옆 칸 그대로"
        );
        for (fmt, doc) in reopened(&core) {
            assert_eq!(
                doc.get_cell_style_at_by_path(0, para, &path).unwrap(),
                style,
                "{name} {fmt}"
            );
        }
    }
}

fn cell_text(
    core: &DocumentCore,
    para: usize,
    path: &[(usize, usize, usize)],
    row: u16,
    col: u16,
) -> String {
    let t = table_at(core, para, path);
    let cell = t
        .cells
        .iter()
        .find(|c| c.row == row && c.col == col)
        .unwrap();
    cell.paragraphs.iter().map(|p| p.text.as_str()).collect()
}

fn fill_2x2(core: &mut DocumentCore, para: usize, path: &Path) {
    for (i, text) in ["가", "나", "다", "라"].iter().enumerate() {
        let mut p = path.clone();
        p.last_mut().unwrap().1 = i;
        core.insert_text_in_cell_by_path(0, para, &p, 0, text)
            .unwrap();
    }
}

/// 표/셀 속성 창의 표 값(안 여백·바깥 여백)을 칸 안 표·머리말 표에 쓴다. 바깥 표는 그대로다.
#[test]
fn table_properties_by_path_reach_inner_and_header_tables() {
    for (core, para, path) in [nested(), header()] {
        let mut core = core;
        let outer_padding = (path.len() > 1
            && matches!(
                core.document().sections[0].paragraphs[para].controls[path[0].0],
                Control::Table(_)
            ))
        .then(|| table_at(&core, para, &path[..1]).padding.left);
        let read: serde_json::Value = serde_json::from_str(
            &core
                .get_table_properties_by_path_native(0, para, &path)
                .unwrap(),
        )
        .unwrap();
        assert!(read["cellZones"].is_array(), "{read}");
        core.set_table_properties_by_path_native(
            0,
            para,
            &path,
            r#"{"paddingLeft":777,"outerLeft":321}"#,
        )
        .unwrap();
        let t = table_at(&core, para, &path);
        assert_eq!((t.padding.left, t.outer_margin_left), (777, 321));
        if let Some(before) = outer_padding {
            assert_eq!(
                table_at(&core, para, &path[..1]).padding.left,
                before,
                "바깥 표"
            );
        }
        for (name, doc) in reopened(&core) {
            let t = table_at(&doc, para, &path);
            assert_eq!((t.padding.left, t.outer_margin_left), (777, 321), "{name}");
        }
    }
}

/// 셀 블록 복사·붙이기, 행/열 바꾸기, 셀 너비를 같게를 칸 안 표에 한다.
#[test]
fn cell_block_commands_by_path_reach_inner_table() {
    let (mut core, para, path) = nested();
    fill_2x2(&mut core, para, &path);
    let outer_before = shape(&core, para, &path[..1]);

    let copied = core
        .copy_table_cells_by_path(0, para, &path, (0, 0, 0, 1))
        .unwrap();
    assert!(copied.contains(r#""text":"가\t나""#), "{copied}");
    let html = core
        .export_table_cells_html_by_path(0, para, &path, (0, 0, 0, 1))
        .unwrap();
    assert!(html.contains('가') && html.contains('나') && !html.contains('다'));
    core.paste_table_cells_by_path(0, para, &path, 1, 0, "overwrite")
        .unwrap();
    assert_eq!(cell_text(&core, para, &path, 1, 0), "가");
    assert_eq!(cell_text(&core, para, &path, 1, 1), "나");

    core.copy_table_cells_transposed_by_path(0, para, &path, (0, 0, 0, 1))
        .unwrap();
    core.paste_table_cells_transposed_by_path(0, para, &path, 0, 1)
        .unwrap();
    assert_eq!(
        cell_text(&core, para, &path, 0, 1),
        "가",
        "가·나 줄이 칸으로"
    );
    core.transpose_table_cells_in_place_by_path(0, para, &path)
        .unwrap();
    assert_eq!(shape(&core, para, &path), (2, 2, 4));

    // 첫 칸을 넓힌 뒤 두 칸 너비를 같게 맞춘다.
    core.resize_table_cells_by_cell_path_native(
        0,
        para,
        &path,
        r#"[{"cellIdx":0,"widthDelta":1500},{"cellIdx":2,"widthDelta":1500}]"#,
    )
    .unwrap();
    let widths = |core: &DocumentCore| {
        let t = table_at(core, para, &path);
        let w = |col| {
            t.cells
                .iter()
                .find(|c| c.row == 0 && c.col == col)
                .unwrap()
                .width
        };
        (w(0), w(1))
    };
    let (w0, w1) = widths(&core);
    assert!(w0 > w1 + 1000, "{w0} {w1}");
    core.equalize_table_cell_sizes_by_path(
        0,
        para,
        &path,
        (0, 0, 1, 1),
        rhwp::model::table::CellSizeAxis::Width,
    )
    .unwrap();
    let (e0, e1) = widths(&core);
    assert!((e0 as i64 - e1 as i64).abs() <= 1, "{e0} {e1}");
    core.equalize_table_cell_sizes_by_path(
        0,
        para,
        &path,
        (0, 0, 1, 1),
        rhwp::model::table::CellSizeAxis::Height,
    )
    .unwrap();

    assert_eq!(
        shape(&core, para, &path[..1]),
        outer_before,
        "바깥 표는 그대로"
    );
    for (name, doc) in reopened(&core) {
        assert_eq!(widths(&doc), (e0, e1), "{name}");
        assert_eq!(
            cell_text(&doc, para, &path, 1, 0),
            cell_text(&core, para, &path, 1, 0),
            "{name}"
        );
    }
}
