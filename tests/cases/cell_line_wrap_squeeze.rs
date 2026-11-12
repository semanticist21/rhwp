//! 관계형. 표 칸의 "한 줄로 입력"(`lineWrap=SQUEEZE`)과 영문 세움.
//! 한/글 12.30 Mac에서 SQUEEZE 칸에 글을 더 넣으면 칸 폭 한 줄(`horzsize` = 칸 안 폭)로
//! 저장하고 넘치는 글은 글자 사이를 좁혀 한 줄에 그린다. 셀 속성 JSON이 이 값을
//! 읽고 쓰며, 바꾼 칸은 강제 줄 나눔에서만 줄을 나눈다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::model::control::Control;
use rhwp::model::table::Table;
use rhwp::wasm_api::HwpDocument;

const LONG: &str = "한 줄로 입력 확인용 긴 글입니다 ABCDEFG 0123456789 끝";

fn table(doc: &HwpDocument) -> &Table {
    match &doc.document().sections[0].paragraphs[0].controls[2] {
        Control::Table(table) => table,
        _ => panic!("새 표"),
    }
}

fn doc_with_long_cells() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.create_table_native(0, 0, 0, 1, 2).unwrap();
    for cell in 0..2 {
        doc.insert_text_in_cell_native(0, 0, 2, cell, 0, 0, LONG)
            .unwrap();
    }
    assert!(table(&doc).cells[1].paragraphs[0].line_segs.len() > 1);
    doc
}

/// SVG 글자 노드의 x 좌표들.
fn text_xs(svg: &str) -> Vec<f64> {
    svg.split("<text")
        .skip(1)
        .filter_map(|chunk| {
            let x = &chunk[chunk.find(" x=\"")? + 4..];
            x[..x.find('"')?].parse().ok()
        })
        .collect()
}

#[test]
fn line_wrap_setter_keeps_squeezed_cell_on_one_line() {
    let mut doc = doc_with_long_cells();
    doc.set_cell_properties_native(0, 0, 2, 1, r#"{"lineWrap":1}"#)
        .unwrap();
    let props = doc.get_cell_properties(0, 0, 2, 1).unwrap();
    assert!(props.contains(r#""lineWrap":1"#), "{props}");
    let cells = &table(&doc).cells;
    assert_eq!(cells[1].line_wrap, 1);
    assert_eq!(
        cells[1].paragraphs[0].line_segs.len(),
        1,
        "한 줄로 입력 칸은 한 줄"
    );
    assert!(
        cells[0].paragraphs[0].line_segs.len() > 1,
        "다른 칸은 그대로 나눈다"
    );

    // 이어 친 글도 한 줄에 남는다.
    doc.insert_text_in_cell_native(0, 0, 2, 1, 0, LONG.chars().count(), "11111111")
        .unwrap();
    let seg = &table(&doc).cells[1].paragraphs[0].line_segs;
    assert_eq!(seg.len(), 1);

    // 넘친 글은 칸 안에 좁혀 그린다: 오른쪽 칸 글자가 표 오른쪽 끝을 넘지 않는다.
    let svg = doc.render_page_svg_native(0).unwrap();
    let xs = text_xs(&svg);
    let right_edge = xs.iter().cloned().fold(f64::MIN, f64::max);
    let page = &doc.document().sections[0].section_def.page_def;
    let table_right = rhwp::renderer::hwpunit_to_px(
        (page.margin_left + page.margin_gutter) as i32 + table(&doc).common.width as i32,
        96.0,
    );
    assert!(
        right_edge < table_right,
        "글자가 칸을 넘었다: {right_edge} ≥ {table_right}"
    );

    // 강제 줄 나눔은 그대로 줄을 나눈다.
    doc.insert_text_in_cell_native(0, 0, 2, 1, 0, 3, "\n")
        .unwrap();
    assert_eq!(table(&doc).cells[1].paragraphs[0].line_segs.len(), 2);

    // 일반으로 되돌리면 다시 폭으로 나눈다.
    doc.set_cell_properties_native(0, 0, 2, 1, r#"{"lineWrap":0}"#)
        .unwrap();
    assert!(table(&doc).cells[1].paragraphs[0].line_segs.len() > 2);
}

#[test]
fn squeeze_and_upright_latin_survive_hwpx_and_hwp() {
    let mut doc = doc_with_long_cells();
    doc.set_cell_properties_native(0, 0, 2, 1, r#"{"lineWrap":1}"#)
        .unwrap();
    doc.set_cell_properties_native(0, 0, 2, 0, r#"{"textDirection":2}"#)
        .unwrap();
    let hwpx = doc.export_hwpx_native().unwrap();
    let hwp = doc.export_hwp_native().unwrap();
    for bytes in [hwpx, hwp] {
        let reopened = HwpDocument::from_bytes(&bytes).unwrap();
        let cells = &table(&reopened).cells;
        assert_eq!(cells[1].line_wrap, 1);
        assert_eq!(cells[1].paragraphs[0].line_segs.len(), 1);
        assert_eq!(cells[0].text_direction, 2, "영문 세움");
    }
}
