//! 글자처럼 둔 높은 개체가 있는 줄의 위 밑줄과 세로쓰기 글의 밑줄 위치.
//!
//! 한컴 12.30(Mac) PDF 기준이다. 20pt 글 사이에 30mm 그림을 둔 줄의 위 밑줄은 그림
//! 위끝 0.02em 위에 긋고, 그림 자리의 글자 모양에 위 밑줄이 있으면 그림 폭에도 잇는다
//! (글자만 있는 줄은 글자 위끝 0.02em 위). 세로 글상자의 아래
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

fn picture_line_doc() -> HwpDocument {
    HwpDocument::from_bytes(include_bytes!(
        "../fixtures/top_underline_object_line/hancom-picture-line.hwp"
    ))
    .unwrap()
}

/// (그림 상자, 글자만 있는 줄의 기준선, 그림 줄의 빨간 선 x 순, 나머지 빨간 선).
fn picture_line(doc: &HwpDocument) -> (BoundingBox, f64, Vec<[f64; 4]>, Vec<[f64; 4]>) {
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
    let (mut object_line, rest): (Vec<[f64; 4]>, Vec<[f64; 4]>) = lines(&svg, RED)
        .into_iter()
        .partition(|l| l[1] > plain_baseline && l[1] < picture.y + picture.height);
    object_line.sort_by(|a, b| a[0].total_cmp(&b[0]));
    (picture, plain_baseline, object_line, rest)
}

#[test]
fn top_underline_rises_to_the_top_of_a_taller_inline_picture() {
    // 한컴 12.30이 저장한 문서: 1문단 "본문위밑줄"(글만), 2문단 "앞글[30mm 그림]뒷글", 모두 20pt 빨간 위 밑줄.
    // 한컴 PDF의 위 밑줄: 1문단 y 131.84px, 2문단 y 174.40px(그림 위끝 174.93px 바로 위)에
    // 앞글 113.44..165.28, 그림 자리 165.12..222.08, 뒷글 221.92..273.76px 세 토막.
    let doc = picture_line_doc();
    let (picture, plain_baseline, object_line, rest) = picture_line(&doc);
    // 한컴은 그림 자리의 글자 모양대로 그림 폭에도 같은 높이로 잇는다(앞글·그림·뒷글).
    assert_eq!(object_line.len(), 3, "앞글·그림·뒷글: {object_line:?}");
    for l in &object_line {
        assert!(
            (l[1] - (picture.y - 0.02 * SIZE)).abs() < 0.05,
            "높은 그림 줄의 위 밑줄은 그림 위끝 바로 위: {l:?}, 그림 {picture:?}"
        );
        assert!((l[1] - 174.40).abs() < 0.1, "한컴 PDF 자리: {l:?}");
    }
    for pair in object_line.windows(2) {
        assert!((pair[1][0] - pair[0][2]).abs() < 0.5, "끊김 없이: {pair:?}");
    }
    let over = object_line[1];
    assert!(
        over[0] <= picture.x + 0.01 && over[2] >= picture.x + picture.width - 0.01,
        "그림 폭 전체: {over:?}, 그림 {picture:?}"
    );
    // 한컴 PDF의 그림 자리 선: x 165.12..222.08px.
    assert!(
        (over[0] - 165.12).abs() < 0.5 && (over[2] - 222.08).abs() < 0.5,
        "한컴 PDF 자리: {over:?}"
    );
    let text_line = rest.iter().find(|l| l[1] < plain_baseline).unwrap();
    assert!(
        (text_line[1] - (plain_baseline - 0.87 * SIZE)).abs() < 0.05,
        "글자만 있는 줄은 그대로: {text_line:?}"
    );
}

#[test]
fn inline_picture_without_top_underline_keeps_its_gap() {
    // 그림 자리의 글자 모양에서 위 밑줄을 빼면 그림 폭에는 긋지 않는다.
    // 서식 범위는 글자 위치라 개체 자리만 고를 수 없다: 1..2는 '글'과 그 뒤 그림 자리
    // (UTF-16 1..10)를 덮고, 2..3은 '뒷'이다.
    let mut doc = picture_line_doc();
    doc.apply_char_format_native(0, 1, 1, 2, r#"{"underline":false}"#)
        .unwrap();
    let (picture, _, object_line, _) = picture_line(&doc);
    assert_eq!(object_line.len(), 2, "앞·뒷글만: {object_line:?}");
    assert!(object_line[0][2] <= picture.x + 0.5, "{object_line:?}");
    // 뒷글은 그림 오른끝에서 바로 시작한다.
    assert!(
        (object_line[1][0] - (picture.x + picture.width)).abs() < 0.5,
        "{object_line:?}"
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
    // 영문은 세로 글에서 눕혀(회전) 놓아도 같은 글자 열의 한 줄에 든다.
    vertical_box(&mut doc, 25512, "세로A위밑줄", "Top", RED);

    let tree = doc.build_page_render_tree(0).unwrap();
    let mut cells = Vec::new();
    collect(
        &tree.root,
        &|n| matches!(&n.node_type, RenderNodeType::TextRun(r) if r.is_vertical && !r.text.is_empty()),
        &mut cells,
    );
    let svg = doc.render_page_svg_native(0).unwrap();
    assert!(
        cells.iter().any(|n| matches!(&n.node_type, RenderNodeType::TextRun(r) if r.text == "A" && r.rotation != 0.0)),
        "눕힌 영문"
    );
    for (color, count, top) in [(BLUE, 6, false), (RED, 6, true)] {
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

/// 위 물결선 꺾은선 노드의 (x, y) 점들과 그 글 run의 (왼끝, 오른끝, 기준선).
fn waves(doc: &HwpDocument, text: &str) -> (Vec<Vec<(f64, f64)>>, (f64, f64, f64)) {
    use rhwp::renderer::PathCommand;
    let tree = doc.build_page_render_tree(0).unwrap();
    let mut lines = Vec::new();
    collect(
        &tree.root,
        &|n| {
            matches!(n.node_type, RenderNodeType::TextLine(_))
                && n.children
                    .iter()
                    .any(|c| matches!(&c.node_type, RenderNodeType::TextRun(r) if r.text == text))
        },
        &mut lines,
    );
    let line = lines[0];
    let run = line
        .children
        .iter()
        .find(|c| matches!(&c.node_type, RenderNodeType::TextRun(r) if r.text == text))
        .unwrap();
    let RenderNodeType::TextRun(r) = &run.node_type else {
        unreachable!()
    };
    let paths = line
        .children
        .iter()
        .filter_map(|c| match &c.node_type {
            RenderNodeType::Path(p) => Some(
                p.commands
                    .iter()
                    .map(|c| match *c {
                        PathCommand::MoveTo(x, y) | PathCommand::LineTo(x, y) => (x, y),
                        _ => panic!("꺾은선만: {c:?}"),
                    })
                    .collect(),
            ),
            _ => None,
        })
        .collect();
    (
        paths,
        (
            run.bbox.x,
            run.bbox.x + run.bbox.width,
            run.bbox.y + r.baseline,
        ),
    )
}

#[test]
fn wave_top_underlines_follow_hancom_print() {
    // 한컴 12.30 PDF(20pt, 다섯 글자): 물결은 마루 1.014em·골 0.894em 위의 꺾은선이며 반 주기
    // 3.2px마다 꺾인다. 글 왼끝에서 마루로 시작해 글 끝을 넘긴 첫 반 주기 끝에서 멈춘다
    // (113.44→244.48px). 이중 물결은 0.924em 마루에 하나 더 긋는다. 선 굵기는 0.8px.
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    let rows = [("물결위밑줄", 11), ("이중물결위", 12)];
    for (index, (text, shape)) in rows.iter().enumerate() {
        if index > 0 {
            doc.split_paragraph_native(0, index - 1, rows[index - 1].0.chars().count(), None)
                .unwrap();
        }
        doc.insert_text_native(0, index, 0, text).unwrap();
        doc.apply_char_format_native(
            0,
            index,
            0,
            text.chars().count(),
            &format!(
                r#"{{"fontSize":2000,"underline":true,"underlineType":"Top","underlineShape":{shape},"underlineColor":"{RED}"}}"#
            ),
        )
        .unwrap();
    }
    for ((text, _), crests) in rows.iter().zip([&[1.014][..], &[1.014, 0.924][..]]) {
        let (paths, (left, right, baseline)) = waves(&doc, text);
        assert_eq!(paths.len(), crests.len(), "{text}: {paths:?}");
        for (points, crest) in paths.iter().zip(crests) {
            let crest_y = baseline - crest * SIZE;
            let half = 0.12 * SIZE;
            assert!((points[0].0 - left).abs() < 1e-6, "글 왼끝에서: {points:?}");
            for (i, &(x, y)) in points.iter().enumerate() {
                assert!(
                    (x - (left + i as f64 * half)).abs() < 1e-6,
                    "반 주기마다: {points:?}"
                );
                let expected = if i % 2 == 0 {
                    crest_y
                } else {
                    crest_y + 0.12 * SIZE
                };
                assert!(
                    (y - expected).abs() < 1e-6,
                    "마루·골: {i} {y} vs {expected}"
                );
            }
            let end = points.last().unwrap().0;
            assert!(
                end >= right - 1e-6 && end < right + half,
                "{end} vs {right}"
            );
            assert!((end - left - 131.04).abs() < 0.2, "한컴 PDF 폭: {end}");
        }
    }
    let svg = doc.render_page_svg_native(0).unwrap();
    assert!(!svg.contains(" Q"), "예전 곡선 물결은 남지 않는다");
}
