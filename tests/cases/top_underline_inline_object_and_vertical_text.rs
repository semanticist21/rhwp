//! 글자처럼 둔 높은 개체가 있는 줄의 위 밑줄과 세로쓰기 글의 밑줄 위치.
//!
//! 한컴 12.30(Mac) PDF 기준이다. 20pt 글 사이에 30mm 그림을 둔 줄의 위 밑줄은 그림
//! 위끝 0.02em 위에 긋는다(글자만 있는 줄은 글자 위끝 0.02em 위). 세로 글상자의 아래
//! 밑줄은 글자 칸 오른쪽 0.02em 밖, 위 밑줄은 왼쪽 0.02em 밖에 글자 열을 따라 한 줄로 긋는다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::renderer::render_tree::{BoundingBox, RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

const RED: &str = "#cc0000";
const BLUE: &str = "#0000cc";
/// 20pt를 96dpi px로 바꾼 글자 크기.
const SIZE: f64 = 20.0 * 96.0 / 72.0;

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!(" {name}=\"");
    let rest = tag.split_once(&marker)?.1;
    rest.split_once('"').map(|(value, _)| value)
}

/// SVG가 그린 그 색 `<line>`의 (x1, y1, x2, y2).
fn lines(svg: &str, color: &str) -> Vec<[f64; 4]> {
    svg.split("<line ")
        .skip(1)
        .filter_map(|chunk| {
            let tag = format!(" {}", chunk.split_once('>').unwrap().0);
            (attr(&tag, "stroke") == Some(color)).then(|| {
                ["x1", "y1", "x2", "y2"].map(|name| attr(&tag, name).unwrap().parse().unwrap())
            })
        })
        .collect()
}

fn collect<'a>(
    node: &'a RenderNode,
    keep: &dyn Fn(&RenderNode) -> bool,
    out: &mut Vec<&'a RenderNode>,
) {
    if keep(node) {
        out.push(node);
    }
    for child in &node.children {
        collect(child, keep, out);
    }
}

fn underline(kind: &str, color: &str) -> String {
    format!(
        r#"{{"fontSize":2000,"underline":true,"underlineType":"{kind}","underlineShape":0,"underlineColor":"{color}"}}"#
    )
}

#[test]
fn top_underline_rises_to_the_top_of_a_taller_inline_picture() {
    // 한컴 12.30이 저장한 문서: 1문단 "본문위밑줄"(글만), 2문단 "앞글[30mm 그림]뒷글", 모두 20pt 빨간 위 밑줄.
    // 한컴 PDF의 위 밑줄: 1문단 y 131.84px, 2문단 y 174.40px(그림 위끝 174.93px 바로 위).
    let doc = HwpDocument::from_bytes(include_bytes!(
        "../fixtures/top_underline_object_line/hancom-picture-line.hwp"
    ))
    .unwrap();
    let tree = doc.build_page_render_tree(0).unwrap();
    let mut images = Vec::new();
    collect(
        &tree.root,
        &|n| matches!(n.node_type, RenderNodeType::Image(_)),
        &mut images,
    );
    assert_eq!(images.len(), 1);
    let picture: BoundingBox = images[0].bbox;
    let mut runs = Vec::new();
    collect(
        &tree.root,
        &|n| matches!(&n.node_type, RenderNodeType::TextRun(r) if r.text == "본문위밑줄"),
        &mut runs,
    );
    let RenderNodeType::TextRun(plain) = &runs[0].node_type else {
        unreachable!()
    };
    let plain_baseline = runs[0].bbox.y + plain.baseline;

    let svg = doc.render_page_svg_native(0).unwrap();
    let drawn = lines(&svg, RED);
    let (object_line, rest): (Vec<&[f64; 4]>, Vec<&[f64; 4]>) = drawn
        .iter()
        .partition(|l| l[1] > plain_baseline && l[1] < picture.y + picture.height);
    assert_eq!(object_line.len(), 2, "그림 앞뒤 글: {drawn:?}");
    for l in object_line {
        assert!(
            (l[1] - (picture.y - 0.02 * SIZE)).abs() < 0.05,
            "높은 그림 줄의 위 밑줄은 그림 위끝 바로 위: {l:?}, 그림 {picture:?}"
        );
        assert!((l[1] - 174.40).abs() < 0.1, "한컴 PDF 자리: {l:?}");
    }
    let text_line = rest.iter().find(|l| l[1] < plain_baseline).unwrap();
    assert!(
        (text_line[1] - (plain_baseline - 0.87 * SIZE)).abs() < 0.05,
        "글자만 있는 줄은 그대로: {text_line:?}"
    );
}

/// 세로 글상자 하나를 만들고 글과 밑줄을 넣는다.
fn vertical_box(doc: &mut HwpDocument, x: u32, text: &str, kind: &str, color: &str) {
    let result: serde_json::Value = serde_json::from_str(
        &doc.create_shape_control_native(
            0,
            0,
            0,
            8504,
            14173,
            x,
            8504,
            false,
            "InFrontOfText",
            "textbox",
            false,
            false,
            &[],
        )
        .unwrap(),
    )
    .unwrap();
    let para = result["paraIdx"].as_u64().unwrap() as usize;
    let control = result["controlIdx"].as_u64().unwrap() as usize;
    doc.set_shape_properties_native(0, para, control, r#"{"tbTextDirection":"Vertical"}"#)
        .unwrap();
    doc.insert_text_in_cell_native(0, para, control, 0, 0, 0, text)
        .unwrap();
    doc.apply_char_format_in_cell_native(
        0,
        para,
        control,
        0,
        0,
        0,
        text.chars().count(),
        &underline(kind, color),
    )
    .unwrap();
}

#[test]
fn vertical_text_underlines_run_beside_the_glyph_column() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    vertical_box(&mut doc, 8504, "세로아래밑줄", "Bottom", BLUE);
    vertical_box(&mut doc, 25512, "세로위밑줄", "Top", RED);

    let tree = doc.build_page_render_tree(0).unwrap();
    let mut cells = Vec::new();
    collect(
        &tree.root,
        &|n| matches!(&n.node_type, RenderNodeType::TextRun(r) if r.is_vertical && !r.text.is_empty()),
        &mut cells,
    );
    let svg = doc.render_page_svg_native(0).unwrap();
    for (color, count, top) in [(BLUE, 6, false), (RED, 5, true)] {
        let column: Vec<BoundingBox> = cells
            .iter()
            .map(|n| n.bbox)
            .filter(|b| if top { b.x > 300.0 } else { b.x < 300.0 })
            .collect();
        assert_eq!(column.len(), count);
        let drawn = lines(&svg, color);
        assert_eq!(drawn.len(), 1, "글자 열을 따라 한 줄: {drawn:?}");
        let [x1, y1, x2, y2] = drawn[0];
        assert!((x1 - x2).abs() < 1e-9, "세로선: {:?}", drawn[0]);
        let side = if top {
            column[0].x - 0.02 * SIZE
        } else {
            column[0].x + column[0].width + 0.02 * SIZE
        };
        assert!((x1 - side).abs() < 0.05, "{x1} vs {side}");
        assert!((y1 - column[0].y).abs() < 0.05);
        let last = column[count - 1];
        assert!((y2 - (last.y + last.height)).abs() < 0.05);
    }
}
