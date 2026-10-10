//! 위 밑줄의 잉크만 본문 clip에 포함하며 글줄·캐럿·선택과 다른 영역의 경계는 보존한다.
//! SVG가 실제로 발행한 선의 중심과 획 두께에서 기대 잉크를 읽는다.
//! 한컴 인쇄와의 시각 일치나 셀·글상자의 위 밑줄 clipping 해결을 주장하지 않는다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::control::Control;
use rhwp::model::paragraph::{CharShapeRef, LineSeg, Paragraph};
use rhwp::model::shape::{HorzRelTo, VertRelTo};
use rhwp::model::style::UnderlineType;
use rhwp::renderer::render_tree::{BoundingBox, RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;
use serde_json::{json, Value};

const TEXTS: [&str; 2] = ["H 첫째 줄", "H 둘째 줄"];
const COLOR: &str = "#d03050";
const EPS: f64 = 1e-8;

fn rect(bbox: BoundingBox) -> [f64; 4] {
    [bbox.x, bbox.y, bbox.width, bbox.height]
}

fn fixture() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, TEXTS[0]).unwrap();
    doc.insert_paragraph_native(0, 1).unwrap();
    doc.insert_text_native(0, 1, 0, TEXTS[1]).unwrap();
    for (para, text) in TEXTS.iter().enumerate() {
        doc.apply_char_format_native(0, para, 0, text.chars().count(), r#"{"fontSize":3600}"#)
            .unwrap();
    }
    assert_eq!(doc.page_count(), 1);
    doc
}

fn decorate(doc: &mut HwpDocument, kind: &str, shape: u8) {
    let props = json!({
        "underlineType": kind,
        "underlineShape": shape,
        "underlineColor": COLOR,
    })
    .to_string();
    for (para, text) in TEXTS.iter().enumerate() {
        doc.apply_char_format_native(0, para, 0, text.chars().count(), &props)
            .unwrap();
    }
}

fn body(node: &RenderNode) -> (&RenderNode, BoundingBox) {
    fn find(node: &RenderNode) -> Option<(&RenderNode, BoundingBox)> {
        if let RenderNodeType::Body {
            clip_rect: Some(clip),
        } = &node.node_type
        {
            return Some((node, *clip));
        }
        node.children.iter().find_map(find)
    }
    find(node).expect("본문 clip")
}

fn geometry(node: &RenderNode) -> Value {
    let detail = match &node.node_type {
        RenderNodeType::TextRun(run) => json!({
            "kind": "run", "text": run.text, "baseline": run.baseline,
            "start": run.char_start, "cell": run.cell_context,
            "positions": run.layout_positions, "display": run.display_text,
        }),
        RenderNodeType::TextLine(line) => json!({"kind": "line", "line": line}),
        RenderNodeType::TableCell(cell) => json!({"kind": "cell", "cell": cell}),
        RenderNodeType::Body { .. } => json!({"kind": "body"}),
        RenderNodeType::Header => json!({"kind": "header"}),
        RenderNodeType::Footer => json!({"kind": "footer"}),
        RenderNodeType::TextBox => json!({"kind": "textbox"}),
        _ => Value::Null,
    };
    json!({
        "bbox": node.bbox, "detail": detail,
        "children": node.children.iter().map(geometry).collect::<Vec<_>>(),
    })
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!(" {name}=\"");
    let rest = tag.split_once(&marker)?.1;
    rest.split_once('"').map(|(value, _)| value)
}

fn underline_inks(svg: &str, color: &str) -> Vec<BoundingBox> {
    svg.split("<line ")
        .skip(1)
        .filter_map(|chunk| {
            let tag = format!(" {}", chunk.split_once('>').unwrap().0);
            if attr(&tag, "stroke") != Some(color) {
                return None;
            }
            let number = |name| attr(&tag, name).unwrap().parse::<f64>().unwrap();
            let (x1, x2, y1, y2, width) = (
                number("x1"),
                number("x2"),
                number("y1"),
                number("y2"),
                number("stroke-width"),
            );
            assert!((y1 - y2).abs() < EPS, "가로 위 밑줄");
            // 기본 butt cap은 길이 방향으로 번지지 않고 원형 점선만 획 반만큼 번진다.
            let end_pad = if attr(&tag, "stroke-linecap") == Some("round") {
                width / 2.0
            } else {
                0.0
            };
            Some(BoundingBox::new(
                x1.min(x2) - end_pad,
                y1 - width / 2.0,
                (x2 - x1).abs() + end_pad * 2.0,
                width,
            ))
        })
        .collect()
}

fn assert_contains(clip: BoundingBox, ink: BoundingBox) {
    assert!(clip.x <= ink.x + EPS, "잉크 좌단: {clip:?} / {ink:?}");
    assert!(
        clip.y <= ink.y + EPS,
        "위 밑줄 상단 잘림: {clip:?} / {ink:?}"
    );
    assert!(clip.x + clip.width + EPS >= ink.x + ink.width, "잉크 우단");
    assert!(
        clip.y + clip.height + EPS >= ink.y + ink.height,
        "잉크 하단"
    );
}

fn interaction(doc: &HwpDocument) -> Value {
    let carets: Vec<Value> = TEXTS
        .iter()
        .enumerate()
        .flat_map(|(para, text)| {
            (0..=text.chars().count()).map(move |offset| {
                serde_json::from_str(&doc.get_cursor_rect_native(0, para, offset).unwrap()).unwrap()
            })
        })
        .collect();
    let selection: Value = serde_json::from_str(
        &doc.get_selection_rects(0, 0, 1, 1, TEXTS[1].chars().count() as u32)
            .unwrap(),
    )
    .unwrap();
    assert!(!selection.as_array().unwrap().is_empty(), "실제 선택 영역");
    json!({"carets": carets, "selection": selection, "pages": doc.page_count()})
}

#[test]
fn first_and_second_36pt_top_underline_fit_without_moving_text_or_interaction() {
    for shape in 0..=10 {
        let mut doc = fixture();
        let before = doc.build_page_render_tree(0).unwrap();
        let (_, plain_clip) = body(&before.root);
        let before_geometry = geometry(&before.root);
        let before_interaction = interaction(&doc);
        decorate(&mut doc, "Top", shape);
        let after = doc.build_page_render_tree(0).unwrap();
        let (_, clip) = body(&after.root);
        let svg = doc.render_page_svg_native(0).unwrap();
        let inks = underline_inks(&svg, COLOR);
        assert!(inks.len() >= 2, "각 줄에 위 밑줄: shape {shape}");
        let first_top = inks.iter().map(|ink| ink.y).fold(f64::INFINITY, f64::min);
        assert!(
            first_top < plain_clip.y,
            "첫 줄 위 밑줄이 기존 clip 위에 있어야 한다"
        );
        assert!(
            inks.iter().any(|ink| ink.y > plain_clip.y),
            "둘째 줄 대조군"
        );
        for ink in inks {
            assert_contains(clip, ink);
        }
        assert!(
            (clip.y - first_top).abs() < EPS,
            "실제 위 밑줄만큼만 상단 확장"
        );
        assert!(
            (clip.y + clip.height - plain_clip.y - plain_clip.height).abs() < EPS,
            "상단 확장이 본문 하단을 바꾸지 않는다"
        );
        assert_eq!(
            geometry(&after.root),
            before_geometry,
            "글줄·글자 bbox 보존: shape {shape}"
        );
        assert_eq!(
            interaction(&doc),
            before_interaction,
            "캐럿·선택·쪽 보존: shape {shape}"
        );
        for (para, expected) in TEXTS.iter().enumerate() {
            assert_eq!(&doc.document().sections[0].paragraphs[para].text, expected);
        }
    }
}

#[test]
fn top_clip_survives_snapshots_and_both_formats_without_changing_content() {
    let mut doc = fixture();
    let plain = doc.save_snapshot_native();
    let plain_clip = body(&doc.build_page_render_tree(0).unwrap().root).1;
    decorate(&mut doc, "Top", 0);
    let top = doc.save_snapshot_native();
    let top_clip = body(&doc.build_page_render_tree(0).unwrap().root).1;
    assert!(top_clip.y < plain_clip.y);
    doc.restore_snapshot_native(plain).unwrap();
    assert_eq!(
        rect(body(&doc.build_page_render_tree(0).unwrap().root).1),
        rect(plain_clip)
    );
    doc.restore_snapshot_native(top).unwrap();
    assert_eq!(
        rect(body(&doc.build_page_render_tree(0).unwrap().root).1),
        rect(top_clip)
    );
    let original = format!("{:?}", doc.document());
    for bytes in [
        doc.export_hwp_with_adapter_snapshot().unwrap(),
        doc.export_hwpx_native().unwrap(),
    ] {
        let saved = HwpDocument::from_bytes(&bytes).unwrap();
        assert_eq!(saved.page_count(), 1);
        let clip = body(&saved.build_page_render_tree(0).unwrap().root).1;
        let inks = underline_inks(&saved.render_page_svg_native(0).unwrap(), COLOR);
        // 영문·한글의 글꼴 슬롯 때문에 같은 줄이 여러 run으로 나뉠 수 있다.
        let mut rows: Vec<_> = inks.iter().map(|ink| ink.y).collect();
        rows.sort_by(f64::total_cmp);
        rows.dedup_by(|a, b| (*a - *b).abs() < EPS);
        assert_eq!(rows.len(), 2, "저장본도 첫 줄과 둘째 줄을 각각 그린다");
        for ink in inks {
            assert_contains(clip, ink);
        }
        for (para, text) in TEXTS.iter().enumerate() {
            assert_eq!(&saved.document().sections[0].paragraphs[para].text, text);
            let props: Value =
                serde_json::from_str(&saved.get_char_properties_at_native(0, para, 1).unwrap())
                    .unwrap();
            assert_eq!(props["fontSize"], 3600);
            assert_eq!(props["underlineType"], "Top");
            assert_eq!(props["underlineShape"], 0);
        }
    }
    assert_eq!(
        format!("{:?}", doc.document()),
        original,
        "렌더·내보내기는 원문을 바꾸지 않는다"
    );
}

#[test]
fn header_top_ink_does_not_expand_body_clip_and_body_top_does_not_move_header() {
    let mut doc = fixture();
    doc.create_header_footer_native(0, true, 0).unwrap();
    doc.insert_text_in_header_footer_native(0, true, 0, 0, 0, "머리말 경계")
        .unwrap();
    doc.apply_char_format_in_header_footer_native(0, true, 0, 0, 0, 0, 6, r#"{"fontSize":3600}"#)
        .unwrap();
    let plain = doc.build_page_render_tree(0).unwrap();
    let plain_clip = body(&plain.root).1;
    doc.apply_char_format_in_header_footer_native(
        0,
        true,
        0,
        0,
        0,
        0,
        6,
        r##"{"underlineType":"Top","underlineColor":"#33aa66"}"##,
    )
    .unwrap();
    let header_top = doc.build_page_render_tree(0).unwrap();
    assert_eq!(
        rect(body(&header_top.root).1),
        rect(plain_clip),
        "머리말 잉크가 본문 clip에 섞이지 않는다"
    );
    let outside = underline_inks(&doc.render_page_svg_native(0).unwrap(), "#33aa66");
    assert_eq!(outside.len(), 1);
    assert!(
        outside[0].y < plain_clip.y,
        "본문보다 위에 있는 실제 머리말"
    );
    let before_interaction = interaction(&doc);
    let before_geometry = geometry(&header_top.root);
    let hf_caret = doc
        .get_cursor_rect_in_header_footer_native(0, true, 0, 0, 2, -1)
        .unwrap();
    decorate(&mut doc, "Top", 0);
    let after = doc.build_page_render_tree(0).unwrap();
    assert_eq!(
        geometry(&after.root),
        before_geometry,
        "머리말/본문의 글줄·상자 보존"
    );
    assert_eq!(interaction(&doc), before_interaction);
    assert_eq!(
        doc.get_cursor_rect_in_header_footer_native(0, true, 0, 0, 2, -1)
            .unwrap(),
        hf_caret
    );
    assert!(
        body(&after.root).1.y > outside[0].y,
        "본문 clip은 머리말까지 전면 확장하지 않는다"
    );
}

#[test]
fn bottom_none_shadow_and_emphasis_keep_the_existing_body_clip_and_geometry() {
    let mut doc = fixture();
    let before = doc.build_page_render_tree(0).unwrap();
    let clip = rect(body(&before.root).1);
    let before_geometry = geometry(&before.root);
    let before_interaction = interaction(&doc);
    for kind in ["None", "Bottom"] {
        for shape in 0..=12 {
            decorate(&mut doc, kind, shape);
            for shadow in 0..=2 {
                doc.apply_char_format_native(
                    0,
                    0,
                    0,
                    TEXTS[0].chars().count(),
                    &json!({"shadowType":shadow,"emphasisDot":6}).to_string(),
                )
                .unwrap();
                let props: Value =
                    serde_json::from_str(&doc.get_char_properties_at_native(0, 0, 1).unwrap())
                        .unwrap();
                assert_eq!(props["underlineType"], kind);
                assert_eq!(props["underlineShape"], shape);
                assert_eq!(props["shadowType"], shadow);
                assert_eq!(props["emphasisDot"], 6);
                let after = doc.build_page_render_tree(0).unwrap();
                assert_eq!(
                    rect(body(&after.root).1),
                    clip,
                    "기존 장식 clip 보존: {kind}/{shape}/{shadow}"
                );
                assert_eq!(geometry(&after.root), before_geometry);
                assert_eq!(interaction(&doc), before_interaction);
            }
        }
    }
}

#[test]
fn clipped_textbox_and_cell_ink_keep_their_clips_and_the_floating_paper_limit() {
    let mut doc = fixture();
    doc.insert_paragraph_native(0, 2).unwrap();
    let created: Value =
        serde_json::from_str(&doc.create_table_native(0, 2, 0, 1, 1).unwrap()).unwrap();
    let parent = created["paraIdx"].as_u64().unwrap() as usize;
    let control = created["controlIdx"].as_u64().unwrap() as usize;
    doc.insert_text_in_cell_native(0, parent, control, 0, 0, 0, "H 셀")
        .unwrap();
    doc.apply_char_format_in_cell_native(0, parent, control, 0, 0, 0, 3, r#"{"fontSize":3600}"#)
        .unwrap();
    doc.create_shape_control_native(
        0,
        0,
        0,
        9_000,
        6_000,
        4_000,
        0,
        false,
        "InFrontOfText",
        "textbox",
        false,
        false,
        &[],
    )
    .unwrap();
    let paper_hu = doc.document().sections[0].section_def.page_def.height;
    doc.create_shape_control_native(
        0,
        0,
        0,
        9_000,
        6_000,
        4_000,
        paper_hu - 3_000,
        false,
        "InFrontOfText",
        "rectangle",
        false,
        false,
        &[],
    )
    .unwrap();
    let mut model = doc.document().clone();
    for control in &mut model.sections[0].paragraphs[0].controls {
        if let Control::Shape(shape) = control {
            shape.common_mut().vert_rel_to = VertRelTo::Para;
            shape.common_mut().horz_rel_to = HorzRelTo::Column;
        }
    }
    // 본문의 첫 raw 모양은 제어 문자에 남은 기본 모양일 수 있다. 실제 36pt 셀 모양을 쓴다.
    let shape_id = match &model.sections[0].paragraphs[parent].controls[control] {
        Control::Table(table) => table.cells[0].paragraphs[0].char_shapes[0].char_shape_id,
        _ => unreachable!("공개 API로 만든 표"),
    };
    let textbox = model.sections[0].paragraphs[0]
        .controls
        .iter_mut()
        .find_map(|control| match control {
            Control::Shape(shape) => shape
                .drawing_mut()
                .and_then(|drawing| drawing.text_box.as_mut()),
            _ => None,
        })
        .unwrap();
    textbox.paragraphs = vec![Paragraph {
        text: "H 상자".to_owned(),
        char_count: 5,
        char_offsets: vec![0, 1, 2, 3],
        char_shapes: vec![CharShapeRef {
            start_pos: 0,
            char_shape_id: shape_id,
        }],
        line_segs: vec![LineSeg {
            line_height: 3_600,
            text_height: 3_600,
            baseline_distance: 3_060,
            segment_width: 8_000,
            tag: LineSeg::TAG_SINGLE_SEGMENT_LINE,
            ..Default::default()
        }],
        ..Default::default()
    }];
    doc.set_document(model.clone());
    let before = doc.build_page_render_tree(0).unwrap();
    let (_, clip) = body(&before.root);
    let paper_height = before.root.bbox.height;
    assert!(
        (clip.y + clip.height - paper_height).abs() < EPS,
        "실제 용지 아래로 나온 부동 개체가 상한에서 잘린다"
    );
    fn overflows(node: &RenderNode, paper_height: f64) -> bool {
        (matches!(
            node.node_type,
            RenderNodeType::Rectangle(_) | RenderNodeType::TextBox
        ) && node.bbox.y + node.bbox.height > paper_height)
            || node
                .children
                .iter()
                .any(|child| overflows(child, paper_height))
    }
    assert!(
        overflows(&before.root, paper_height),
        "용지 하단을 넘는 부동 개체 대조군"
    );
    fn clips(node: &RenderNode, result: &mut Vec<Value>) {
        match &node.node_type {
            RenderNodeType::TextBox => result.push(json!({"kind":"textbox", "bbox":node.bbox})),
            RenderNodeType::TableCell(cell) if cell.clip => {
                result.push(json!({"kind":"cell", "bbox":node.bbox, "cell":cell}))
            }
            _ => {}
        }
        for child in &node.children {
            clips(child, result);
        }
    }
    let mut original_clips = Vec::new();
    clips(&before.root, &mut original_clips);
    assert!(original_clips.iter().any(|clip| clip["kind"] == "textbox"));
    assert!(original_clips.iter().any(|clip| clip["kind"] == "cell"));
    let mut top_shape = model.doc_info.char_shapes[shape_id as usize].clone();
    top_shape.underline_type = UnderlineType::Top;
    top_shape.underline_color = 0x5030d0;
    top_shape.raw_data = None;
    let top_id = model.doc_info.char_shapes.len() as u32;
    model.doc_info.char_shapes.push(top_shape);
    for paragraph in &mut model.sections[0].paragraphs {
        for control in &mut paragraph.controls {
            match control {
                Control::Shape(shape) => {
                    if let Some(textbox) = shape
                        .drawing_mut()
                        .and_then(|drawing| drawing.text_box.as_mut())
                    {
                        textbox.paragraphs[0].char_shapes[0].char_shape_id = top_id;
                    }
                }
                Control::Table(table) => {
                    table.cells[0].paragraphs[0].char_shapes[0].char_shape_id = top_id
                }
                _ => {}
            }
        }
    }
    let before_geometry = geometry(&before.root);
    let before_interaction = interaction(&doc);
    doc.set_document(model);
    let after = doc.build_page_render_tree(0).unwrap();
    let mut after_clips = Vec::new();
    clips(&after.root, &mut after_clips);
    assert_eq!(
        after_clips, original_clips,
        "셀·글상자의 실제 clip은 유지한다"
    );
    assert_eq!(
        rect(body(&after.root).1),
        rect(clip),
        "하위 clip 밖의 위 밑줄로 본문 clip을 넓히지 않는다"
    );
    assert_eq!(geometry(&after.root), before_geometry);
    assert_eq!(interaction(&doc), before_interaction);
    let textbox_top = original_clips
        .iter()
        .filter(|clip| clip["kind"] == "textbox")
        .map(|clip| clip["bbox"]["y"].as_f64().unwrap())
        .fold(f64::INFINITY, f64::min);
    assert!(
        underline_inks(&doc.render_page_svg_native(0).unwrap(), COLOR)
            .iter()
            .any(|ink| ink.y < textbox_top),
        "실제로 글상자 clip 위에 있는 Top 잉크"
    );
}
