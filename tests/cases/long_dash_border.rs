//! 긴 파선은 일반 파선과 구별하며 쪽·문단·글자·셀의 공통 선 표현을 유지한다.
//! 24/8 × 획 굵기는 저장소 issue-124의 한컴 웹기안기 분석 기록에 따른 계약이다.
//! 이 합성 검사는 실제 한컴 인쇄 출력과의 시각 일치를 주장하지 않는다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::{
    document_core::DocumentCore,
    model::control::Control,
    paint::{LayerNode, PageLayerTree, PaintOp},
    renderer::render_tree::{LineNode, RenderNode, RenderNodeType},
    renderer::{
        canvaskit_policy::{analyze_canvaskit_replay_plan, CanvasKitReplayMode},
        layer_renderer::LayerRenderer,
        long_dash_intervals,
        render_tree::{BoundingBox, PathNode, RectangleNode},
        svg_layer::SvgLayerRenderer,
        LineStyle, PathCommand, ShapeStyle, StrokeDash,
    },
    wasm_api::HwpDocument,
};
use serde_json::{json, Value};

const COLOR: u32 = 0x4433cc;

fn blank() -> DocumentCore {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    let mut core = DocumentCore::new_empty();
    core.set_document(doc.document().clone());
    core
}

#[test]
fn long_dash_replay_is_bounded_and_keeps_direction_endpoints_and_legacy_dash() {
    assert_eq!(long_dash_intervals(1.0), Some([24.0, 8.0]));
    for width in [
        0.0,
        -1.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MAX,
    ] {
        assert_eq!(long_dash_intervals(width), None);
    }
    let bbox = BoundingBox::new(0.0, 0.0, 120.0, 120.0);
    let style = LineStyle {
        color: COLOR,
        width: 0.80,
        dash: StrokeDash::LongDash,
        ..Default::default()
    };
    let shape = ShapeStyle {
        stroke_color: Some(COLOR),
        stroke_width: 0.80,
        stroke_dash: StrokeDash::LongDash,
        ..Default::default()
    };
    let ops = vec![
        PaintOp::line(bbox, LineNode::new(10.0, 10.0, 15.0, 10.0, style.clone())),
        PaintOp::line(bbox, LineNode::new(80.0, 10.0, 10.0, 10.0, style.clone())),
        PaintOp::line(bbox, LineNode::new(10.0, 20.0, 10.0, 90.0, style.clone())),
        PaintOp::line(bbox, LineNode::new(10.0, 20.0, 70.0, 100.0, style.clone())),
        PaintOp::line(bbox, LineNode::new(10.0, 20.0, 1e12, 20.0, style)),
        PaintOp::rectangle(bbox, RectangleNode::new(0.0, shape.clone(), None)),
        PaintOp::path(
            bbox,
            PathNode::new(
                vec![
                    PathCommand::MoveTo(10.0, 10.0),
                    PathCommand::LineTo(110.0, 110.0),
                ],
                shape,
                None,
            ),
        ),
        PaintOp::line(
            bbox,
            LineNode::new(
                10.0,
                50.0,
                100.0,
                50.0,
                LineStyle {
                    width: 3.84,
                    dash: StrokeDash::Dash,
                    ..Default::default()
                },
            ),
        ),
    ];
    let tree = PageLayerTree::new(120.0, 120.0, LayerNode::leaf(bbox, None, ops));
    let plan = analyze_canvaskit_replay_plan(&tree, CanvasKitReplayMode::Default);
    assert_eq!(plan.summary.direct_items, 8);
    assert_eq!(plan.summary.unsupported_items, 0);
    let json = tree.to_json();
    assert_eq!(json.matches("\"type\":\"line\"").count(), 6);
    assert_eq!(json.matches("longDash").count(), 7);
    assert!(
        json.len() < 10_000,
        "선 길이 때문에 primitive나 JSON 크기가 늘지 않는다"
    );
    let mut renderer = SvgLayerRenderer::new();
    renderer.render_page(&tree).unwrap();
    let svg = renderer.output();
    let patterned: Vec<_> = svg
        .split('<')
        .filter(|tag| tag.contains("stroke=\"#cc3344\""))
        .collect();
    assert_eq!(patterned.len(), 7);
    for tag in patterned {
        assert_svg_pattern(tag, 19.2, 6.4);
    }
    assert_eq!(svg.matches("stroke-dasharray=\"6 3\"").count(), 1);
    for endpoints in [
        "x1=\"10\" y1=\"10\" x2=\"15\" y2=\"10\"",
        "x1=\"80\" y1=\"10\" x2=\"10\" y2=\"10\"",
        "x1=\"10\" y1=\"20\" x2=\"10\" y2=\"90\"",
        "x1=\"10\" y1=\"20\" x2=\"70\" y2=\"100\"",
    ] {
        assert!(svg.contains(endpoints), "선 방향·끝점 보존: {endpoints}");
    }
    assert!(!svg.contains("stroke-dashoffset"), "기존 시작 위상은 0이다");
}

#[test]
fn existing_shape_and_column_code_six_uses_long_dash_without_geometry_changes() {
    for kind in ["line", "rectangle", "ellipse"] {
        let mut doc = blank();
        doc.insert_text_native(0, 0, 0, "도형 뒤 원문").unwrap();
        let created: Value = serde_json::from_str(
            &doc.create_shape_control_native(
                0,
                0,
                0,
                9000,
                6000,
                12000,
                16000,
                false,
                "InFrontOfText",
                kind,
                false,
                false,
                &[],
            )
            .unwrap(),
        )
        .unwrap();
        let parent = created["paraIdx"].as_u64().unwrap() as usize;
        let control = created["controlIdx"].as_u64().unwrap() as usize;
        let props = json!({ "lineType": 2, "borderWidth": 144, "borderColor": COLOR }).to_string();
        doc.set_shape_properties_native(0, parent, control, &props)
            .unwrap();
        let before = doc.get_shape_properties_native(0, parent, control).unwrap();
        let caret = doc.get_cursor_rect_native(0, 0, 2).unwrap();
        doc.set_shape_properties_native(0, parent, control, r#"{"lineType":6}"#)
            .unwrap();
        let mut before: Value = serde_json::from_str(&before).unwrap();
        before["lineType"] = json!(6);
        before["borderAttr"] = json!((before["borderAttr"].as_u64().unwrap() & !0x3f) | 6);
        assert_eq!(
            serde_json::from_str::<Value>(
                &doc.get_shape_properties_native(0, parent, control).unwrap()
            )
            .unwrap(),
            before
        );
        assert_eq!(doc.get_cursor_rect_native(0, 0, 2).unwrap(), caret);
        let layer = doc.get_page_layer_tree_native(0).unwrap();
        assert!(layer.contains("longDash"), "{kind} 원본 code6을 소비한다");
        let svg = doc.render_page_svg_native(0).unwrap();
        let tags: Vec<_> = svg
            .split('<')
            .filter(|tag| tag.contains("stroke=\"#cc3344\""))
            .collect();
        assert_eq!(tags.len(), 1, "{kind}: {svg}");
        assert_svg_pattern(tags[0], 46.08, 15.36);
    }

    let mut doc = blank();
    doc.insert_text_native(0, 0, 0, "단 구분선 보호").unwrap();
    doc.set_column_def_native(0, 2, 0, true, 1200).unwrap();
    let mut model = doc.document().clone();
    let column = model.sections[0]
        .paragraphs
        .iter_mut()
        .flat_map(|para| &mut para.controls)
        .find_map(|control| match control {
            Control::ColumnDef(column) => Some(column),
            _ => None,
        })
        .unwrap();
    column.separator_type = 2;
    column.separator_width = 3;
    column.separator_color = COLOR;
    doc.set_document(model.clone());
    let before = lines(&doc, COLOR);
    let caret = doc.get_cursor_rect_native(0, 0, 2).unwrap();
    for para in &mut model.sections[0].paragraphs {
        for control in &mut para.controls {
            if let Control::ColumnDef(column) = control {
                column.separator_type = 6;
            }
        }
    }
    doc.set_document(model);
    let after = assert_long_dash(&doc, COLOR, 0.80);
    assert_eq!(after.len(), before.len());
    for (old, new) in before.iter().zip(&after) {
        assert_eq!(
            (old.x1, old.y1, old.x2, old.y2),
            (new.x1, new.y1, new.x2, new.y2)
        );
    }
    assert_eq!(doc.get_cursor_rect_native(0, 0, 2).unwrap(), caret);
    let svg = doc.render_page_svg_native(0).unwrap();
    let tags: Vec<_> = svg
        .split('<')
        .filter(|tag| tag.contains("stroke=\"#cc3344\""))
        .collect();
    assert!(!tags.is_empty());
    for tag in tags {
        assert_svg_pattern(tag, 19.2, 6.4);
    }
}

#[cfg(feature = "native-skia")]
#[test]
fn native_skia_long_dash_uses_the_same_phase_and_single_width_scale() {
    use rhwp::renderer::{layer_renderer::RasterRenderOptions, skia::SkiaLayerRenderer};
    for width in [0.32_f64, 0.80, 1.92, 3.84] {
        for (dash, expected_on, expected_gap) in [
            (StrokeDash::LongDash, 24.0 * width, 8.0 * width),
            (StrokeDash::Dash, 6.0 * width.max(1.0), 3.0 * width.max(1.0)),
        ] {
            let bbox = BoundingBox::new(0.0, 0.0, 400.0, 40.0);
            let line = LineNode::new(
                10.0,
                20.5,
                390.0,
                20.5,
                LineStyle {
                    color: COLOR,
                    width,
                    dash,
                    ..Default::default()
                },
            );
            let tree = PageLayerTree::new(
                400.0,
                40.0,
                LayerNode::leaf(bbox, None, vec![PaintOp::line(bbox, line)]),
            );
            let output = SkiaLayerRenderer::new()
                .render_raster_with_options(&tree, RasterRenderOptions::default())
                .unwrap();
            let pixels = image::load_from_memory(&output.bytes).unwrap().to_rgba8();
            let on1 = (10.0 + expected_on * 0.5) as u32;
            let gap = (10.0 + expected_on + expected_gap * 0.5) as u32;
            let on2 = (10.0 + expected_on + expected_gap + expected_on * 0.5) as u32;
            assert!(
                pixels.get_pixel(on1, 20)[3] > 20,
                "{dash:?}/{width}: 처음 dash"
            );
            assert_eq!(pixels.get_pixel(gap, 20)[3], 0, "{dash:?}/{width}: gap");
            assert!(
                pixels.get_pixel(on2, 20)[3] > 20,
                "{dash:?}/{width}: 두 번째 dash"
            );
        }
    }
}

fn border_props(kind: u8, width: u8, color: &str) -> Value {
    let border = json!({ "type": kind, "width": width, "color": color });
    json!({
        "borderLeft": border, "borderRight": border,
        "borderTop": border, "borderBottom": border,
        "fillType": "none"
    })
}

fn page_props(kind: u8, width: u8) -> String {
    let mut props = border_props(kind, width, "#cc3344");
    props["basis"] = json!("paper");
    props["spacingLeft"] = json!(1200);
    props["spacingRight"] = json!(1200);
    props["spacingTop"] = json!(1200);
    props["spacingBottom"] = json!(1200);
    props.to_string()
}

fn collect_lines(node: &RenderNode, color: u32, out: &mut Vec<LineNode>) {
    if let RenderNodeType::Line(line) = &node.node_type {
        if line.style.color == color {
            out.push(line.clone());
        }
    }
    for child in &node.children {
        collect_lines(child, color, out);
    }
}

fn lines(doc: &DocumentCore, color: u32) -> Vec<LineNode> {
    let mut out = Vec::new();
    for page in 0..doc.page_count() {
        collect_lines(
            &doc.build_page_render_tree(page).unwrap().root,
            color,
            &mut out,
        );
    }
    out
}

fn assert_long_dash(doc: &DocumentCore, color: u32, expected_width: f64) -> Vec<LineNode> {
    let found = lines(doc, color);
    assert!(
        !found.is_empty(),
        "대상 테두리가 실제 렌더 트리에 있어야 한다"
    );
    for line in &found {
        assert_eq!(format!("{:?}", line.style.dash), "LongDash");
        assert!((line.style.width - expected_width).abs() < 1e-9);
        assert_eq!(
            line.control_index, None,
            "테두리는 선택 가능한 도형이 아니다"
        );
    }
    found
}

fn assert_svg_pattern(tag: &str, on: f64, gap: f64) {
    let values: Vec<f64> = tag
        .split("stroke-dasharray=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    assert_eq!(values.len(), 2);
    assert!((values[0] - on).abs() < 1e-9, "dash: {tag}");
    assert!((values[1] - gap).abs() < 1e-9, "gap: {tag}");
}

fn number_attr(tag: &str, attr: &str) -> f64 {
    let key = format!("{attr}=\"");
    let value = tag.split(&key).nth(1).unwrap().split('"').next().unwrap();
    value.parse().unwrap()
}

#[test]
fn page_long_dash_scales_with_all_sixteen_stored_border_widths() {
    // #6913의 독립 600dpi 굵기 축이며 새 dash 구현에서 기대를 가져오지 않는다.
    let widths = [
        0.32, 0.48, 0.64, 0.80, 0.96, 1.12, 1.44, 1.92, 2.24, 2.72, 3.84, 5.60, 7.52, 11.36, 15.20,
        18.88,
    ];
    for (index, width) in widths.into_iter().enumerate() {
        let mut doc = blank();
        doc.set_page_border_fill_native(0, &page_props(6, index as u8))
            .unwrap();
        assert_eq!(assert_long_dash(&doc, COLOR, width).len(), 4);
        let svg = doc.render_page_svg_native(0).unwrap();
        let tags: Vec<_> = svg
            .split('<')
            .filter(|tag| tag.starts_with("line ") && tag.contains("stroke=\"#cc3344\""))
            .collect();
        assert_eq!(tags.len(), 4);
        for tag in tags {
            assert!((number_attr(tag, "stroke-width") - width).abs() < 1e-9);
            let values: Vec<f64> = tag
                .split("stroke-dasharray=\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .split_whitespace()
                .map(|value| value.parse().unwrap())
                .collect();
            assert_eq!(values.len(), 2);
            assert!((values[0] - width * 24.0).abs() < 1e-9);
            assert!((values[1] - width * 8.0).abs() < 1e-9);
            assert!(
                !tag.contains("stroke-dashoffset"),
                "각 원래 선의 시작에서 위상 0"
            );
        }
        let layer = doc.get_page_layer_tree_native(0).unwrap();
        assert_eq!(layer.matches("\"dash\":\"longDash\"").count(), 4);
    }
}

#[test]
fn paragraph_character_cell_and_diagonal_borders_share_long_dash_without_moving_text() {
    let mut doc = blank();
    doc.insert_text_native(0, 0, 0, "본문 보호").unwrap();
    let created: Value =
        serde_json::from_str(&doc.create_table_native(0, 0, 5, 1, 1).unwrap()).unwrap();
    let parent = created["paraIdx"].as_u64().unwrap() as usize;
    let control = created["controlIdx"].as_u64().unwrap() as usize;
    doc.insert_text_in_cell_native(0, parent, control, 0, 0, 0, "셀 보호")
        .unwrap();
    let caret_before = doc.get_cursor_rect_native(0, 0, 2).unwrap();
    doc.apply_char_format_native(0, 0, 0, 2, &border_props(6, 3, "#cc3344").to_string())
        .unwrap();
    doc.apply_para_format_native(0, 0, &border_props(6, 3, "#3377aa").to_string())
        .unwrap();
    let mut cell = border_props(6, 3, "#993377");
    cell["diagonalLine"] = json!(6);
    cell["diagonalSlash"] = json!(1);
    cell["diagonalBackSlash"] = json!(1);
    cell["diagonalWidth"] = json!(3);
    cell["diagonalColor"] = json!("#11aa55");
    doc.set_cell_properties_native(0, parent, control, 0, &cell.to_string())
        .unwrap();
    assert_long_dash(&doc, COLOR, 0.80);
    assert_long_dash(&doc, 0xaa7733, 0.80);
    assert_long_dash(&doc, 0x773399, 0.80);
    let diagonal = assert_long_dash(&doc, 0x55aa11, 0.80);
    assert_eq!(diagonal.len(), 2);
    assert!(diagonal.iter().any(|line| line.y1 < line.y2));
    assert!(diagonal.iter().any(|line| line.y1 > line.y2));
    assert!(diagonal.iter().all(|line| line.x1 < line.x2));
    assert_eq!(doc.get_cursor_rect_native(0, 0, 2).unwrap(), caret_before);
    assert_eq!(doc.document().sections[0].paragraphs[0].text, "본문 보호");
    assert_eq!(doc.page_count(), 1);
}

#[test]
fn page_border_snapshot_and_both_formats_preserve_long_dash_and_body_content() {
    let mut doc = blank();
    doc.insert_text_native(0, 0, 0, "긴 파선 저장 보호")
        .unwrap();
    doc.set_page_border_fill_native(0, &page_props(2, 3))
        .unwrap();
    let dash_snapshot = doc.save_snapshot_native();
    let caret = doc.get_cursor_rect_native(0, 0, 3).unwrap();
    doc.set_page_border_fill_native(0, &page_props(6, 3))
        .unwrap();
    let long_snapshot = doc.save_snapshot_native();
    let model = format!("{:?}", doc.document());
    assert_eq!(assert_long_dash(&doc, COLOR, 0.80).len(), 4);
    assert_eq!(doc.get_cursor_rect_native(0, 0, 3).unwrap(), caret);
    doc.restore_snapshot_native(dash_snapshot).unwrap();
    assert!(lines(&doc, COLOR)
        .iter()
        .all(|line| format!("{:?}", line.style.dash) == "Dash"));
    doc.restore_snapshot_native(long_snapshot).unwrap();
    assert_eq!(format!("{:?}", doc.document()), model);
    for bytes in [
        doc.export_hwp_with_adapter_snapshot().unwrap(),
        doc.export_hwpx_native().unwrap(),
    ] {
        let reopened = HwpDocument::from_bytes(&bytes).unwrap();
        assert_eq!(assert_long_dash(&reopened, COLOR, 0.80).len(), 4);
        let props: Value =
            serde_json::from_str(&reopened.get_page_border_fill_native(0).unwrap()).unwrap();
        for edge in ["borderLeft", "borderRight", "borderTop", "borderBottom"] {
            assert_eq!(props[edge]["type"], 6);
            assert_eq!(props[edge]["width"], 3);
        }
        assert_eq!(
            reopened.document().sections[0].paragraphs[0].text,
            "긴 파선 저장 보호"
        );
        assert_eq!(reopened.page_count(), 1);
    }
    assert_eq!(
        format!("{:?}", doc.document()),
        model,
        "렌더·저장은 live 모델을 바꾸지 않는다"
    );
}
