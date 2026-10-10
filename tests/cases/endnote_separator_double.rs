//! 미주 이중 구분선은 공통 두 Single 선과 같은 잉크 높이를 예약한다.
//! NativeSkia 픽셀과 한컴 인쇄 대조는 이 공개 SVG·IR 검사의 범위가 아니다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::{
    model::control::Control,
    paint::{
        layer_tree::{LayerNode, LayerNodeKind},
        paint_op::PaintOp,
        RenderProfile,
    },
    renderer::{
        render_tree::{BoundingBox, LineNode, RenderNode, RenderNodeType},
        LineRenderType, StrokeDash,
    },
    wasm_api::HwpDocument,
};
use serde_json::{json, Value};

const BODY: &str = "본문 앞과 뒤를 보존합니다";
const NOTE: &str = "미주 내용과 캐럿을 보존합니다";
const COLOR: u32 = 0x5030d0;
const CSS_COLOR: &str = "#d03050";

fn value(json: &str) -> Value {
    serde_json::from_str(json).unwrap()
}

fn close(actual: f64, expected: f64, label: &str) {
    assert!(
        (actual - expected).abs() < 0.002,
        "{label}: {actual} != {expected}"
    );
}

fn caret_delta(actual: f64, expected: f64) {
    // 공개 캐럿 JSON은 y를 소수점 한 자리로 각각 반올림한다. 두 값의 차이에는
    // 최대 0.1px 오차가 생기지만 실제 IR·SVG의 잉크 좌표는 위의 정밀 검사로 보호한다.
    assert!(
        (actual - expected).abs() <= 0.100_001,
        "공개 캐럿의 실제 줄 이동: {actual} != {expected}"
    );
}

fn fixture(width: u8) -> (HwpDocument, usize) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.set_page_def_native(
        0,
        &json!({
            "width":22500,"height":15000,"marginLeft":750,"marginRight":750,
            "marginTop":750,"marginBottom":750,"marginHeader":0,"marginFooter":0,
        })
        .to_string(),
    )
    .unwrap();
    doc.insert_text_native(0, 0, 0, BODY).unwrap();
    let inserted = value(
        &doc.insert_endnote_native(0, 0, BODY.chars().count())
            .unwrap(),
    );
    let control = inserted["controlIdx"].as_u64().unwrap() as usize;
    // 새 미주의 번호와 간격 뒤에 실제 내용을 넣는다.
    doc.insert_text_in_footnote_native(0, 0, control, 0, 2, NOTE)
        .unwrap();
    doc.apply_endnote_shape_native(
        0,
        &json!({
            "startNumber":3,"prefixChar":"[","suffixChar":"]",
            "separatorLineType":1,"separatorLineWidth":width,"separatorLength":6000,
            "separatorColor":CSS_COLOR,"separatorMarginTop":0,"separatorMarginBottom":0,
            "noteSpacing":0,
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(doc.page_count(), 1);
    (doc, control)
}

fn lines(doc: &HwpDocument) -> Vec<LineNode> {
    fn collect(node: &RenderNode, result: &mut Vec<LineNode>) {
        if let RenderNodeType::Line(line) = &node.node_type {
            if line.style.color == COLOR {
                result.push(line.clone());
            }
        }
        for child in &node.children {
            collect(child, result);
        }
    }
    let mut result = Vec::new();
    for page in 0..doc.page_count() {
        collect(&doc.build_page_render_tree(page).unwrap().root, &mut result);
    }
    result.sort_by(|a, b| a.y1.total_cmp(&b.y1));
    result
}

fn first_line(doc: &HwpDocument, text: &str) -> BoundingBox {
    fn text_in(node: &RenderNode, result: &mut String) {
        if let RenderNodeType::TextRun(run) = &node.node_type {
            result.push_str(&run.text);
        }
        for child in &node.children {
            text_in(child, result);
        }
    }
    fn find(node: &RenderNode, text: &str) -> Option<BoundingBox> {
        if matches!(node.node_type, RenderNodeType::TextLine(_)) {
            let mut rendered = String::new();
            text_in(node, &mut rendered);
            if rendered.contains(text) {
                return Some(node.bbox);
            }
        }
        node.children.iter().find_map(|child| find(child, text))
    }
    (0..doc.page_count())
        .find_map(|page| find(&doc.build_page_render_tree(page).unwrap().root, text))
        .expect("본문과 미주의 실제 첫 줄")
}

fn model_content(doc: &HwpDocument, control: usize) -> Value {
    let para = &doc.document().sections[0].paragraphs[0];
    let Control::Endnote(note) = &para.controls[control] else {
        panic!("미주 유지");
    };
    assert_eq!(para.text.matches(BODY).count(), 1);
    assert_eq!(note.paragraphs.len(), 1);
    assert_eq!(note.paragraphs[0].text.matches(NOTE).count(), 1);
    assert_eq!(note.number, 3);
    json!({"body":para.text,"bodyShapes":para.char_shapes,"note":note.paragraphs[0].text,
        "noteShapes":note.paragraphs[0].char_shapes,"number":note.number,
        "before":note.before_decoration_letter,"after":note.after_decoration_letter,
        "numberShape":note.number_shape})
}

fn body_interaction(doc: &HwpDocument) -> Value {
    let selection = value(&doc.get_selection_rects(0, 0, 1, 0, 6).unwrap());
    assert!(!selection.as_array().unwrap().is_empty());
    json!({"carets":(0..=BODY.chars().count()).map(|offset|
        value(&doc.get_cursor_rect_native(0,0,offset).unwrap())).collect::<Vec<_>>(),
        "selection":selection})
}

fn note_carets(doc: &HwpDocument, control: usize) -> Vec<Value> {
    (0..=NOTE.chars().count())
        .map(|offset| {
            value(
                &doc.get_cursor_rect_in_note_native(0, 0, control, 0, offset + 2)
                    .unwrap(),
            )
        })
        .collect()
}

fn geometry(doc: &HwpDocument) -> Value {
    fn node_geometry(node: &RenderNode) -> Value {
        json!({"bbox":node.bbox,"kind":node.node_type,
            "children":node.children.iter().map(node_geometry).collect::<Vec<_>>()})
    }
    json!((0..doc.page_count())
        .map(|page| node_geometry(&doc.build_page_render_tree(page).unwrap().root))
        .collect::<Vec<_>>())
}

fn attribute(tag: &str, name: &str) -> f64 {
    tag.split_once(&format!("{name}=\""))
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn assert_double(doc: &HwpDocument, width: u8) -> Vec<LineNode> {
    // HWP 굵기표의 600dpi 양자화 값으로 계산한다. 내부 helper를 기대값에 사용하지 않는다.
    let raw_width: f64 = match width {
        0 => 0.32,
        1 => 0.48,
        5 => 1.12,
        15 => 18.88,
        _ => panic!("fixture 굵기"),
    };
    let span = raw_width.max(3.0);
    let result = lines(doc);
    assert_eq!(result.len(), 2, "이중선은 두 실제 노드");
    for line in &result {
        assert_eq!(
            line.style.line_type,
            LineRenderType::Single,
            "모든 백엔드의 공통 단일선"
        );
        assert_eq!(line.style.dash, StrokeDash::Solid);
        assert_eq!(line.style.color, COLOR);
        close(line.style.width, span * 0.3, "각 획 굵기");
        close(line.y1, line.y2, "수평선");
        close(line.x2 - line.x1, 80.0, "원래 구분선 길이");
    }
    close(result[0].x1, result[1].x1, "같은 시작점");
    close(result[0].x2, result[1].x2, "같은 끝점");
    close(result[1].y1 - result[0].y1, span * 0.7, "두 선 중심 간격");
    let ink_top = result[0].ink_bbox().y;
    let lower = result[1].ink_bbox();
    close(lower.y + lower.height - ink_top, span, "실제 잉크 높이");
    let note = first_line(doc, NOTE);
    assert!(
        note.y + 0.002 >= lower.y + lower.height,
        "여백0에서 미주 첫 줄과 선이 겹치지 않는다"
    );
    for profile in [RenderProfile::Screen, RenderProfile::Print] {
        let svg = doc
            .render_page_svg_layer_with_profile_native(0, profile)
            .unwrap();
        let mut tags = svg
            .split('<')
            .filter(|tag| tag.contains(&format!("stroke=\"{CSS_COLOR}\"")))
            .collect::<Vec<_>>();
        tags.sort_by(|a, b| attribute(a, "y1").total_cmp(&attribute(b, "y1")));
        assert_eq!(tags.len(), 2, "{profile:?}: 두 실제 SVG 획");
        for (tag, line) in tags.iter().zip(&result) {
            assert!(!tag.contains("stroke-dasharray"));
            for (key, expected) in [
                ("x1", line.x1),
                ("x2", line.x2),
                ("y1", line.y1),
                ("y2", line.y2),
                ("stroke-width", line.style.width),
            ] {
                close(
                    attribute(tag, key),
                    expected,
                    &format!("{profile:?}: {key}"),
                );
            }
        }
        fn collect(node: &LayerNode, result: &mut Vec<LineNode>) {
            match &node.kind {
                LayerNodeKind::Group { children, .. } => {
                    for child in children {
                        collect(child, result);
                    }
                }
                LayerNodeKind::ClipRect { child, .. } => collect(child, result),
                LayerNodeKind::Leaf { ops } => {
                    for op in ops {
                        if let PaintOp::Line { line, .. } = op {
                            if line.style.color == COLOR {
                                result.push((**line).clone());
                            }
                        }
                    }
                }
            }
        }
        let mut replay = Vec::new();
        collect(
            &doc.build_page_layer_tree_with_profile(0, profile)
                .unwrap()
                .root,
            &mut replay,
        );
        replay.sort_by(|a, b| a.y1.total_cmp(&b.y1));
        assert_eq!(
            json!(replay),
            json!(result),
            "{profile:?}: Canvas·SVG·Skia가 받는 공통 IR"
        );
    }
    result
}

#[test]
fn double_endnote_separator_reserves_exact_visual_span_with_zero_margins() {
    for width in [0, 1, 5, 15] {
        let (mut doc, control) = fixture(width);
        let original = model_content(&doc, control);
        let body = body_interaction(&doc);
        let old_note = first_line(&doc, NOTE);
        let old_carets = note_carets(&doc, control);
        let single = lines(&doc);
        assert_eq!(single.len(), 1);
        if width == 0 {
            close(single[0].style.width, 0.32, "0.1mm 단일선의 실제 획");
            close(single[0].ink_bbox().height, 0.32, "0.1mm 단일선 잉크 높이");
        }
        doc.apply_endnote_shape_native(0, r#"{"separatorLineType":8}"#)
            .unwrap();
        let double = assert_double(&doc, width);
        let ink_top = double[0].ink_bbox().y;
        close(
            ink_top,
            single[0].y1,
            "선 묶음이 원래 예약 시작 아래에 놓인다",
        );
        let bottom = double[1].ink_bbox();
        let new_note = first_line(&doc, NOTE);
        close(
            new_note.y,
            old_note.y.max(bottom.y + bottom.height),
            "필요한 이중선 높이만 미주 첫 줄을 민다",
        );
        let delta = new_note.y - old_note.y;
        for (old, new) in old_carets.iter().zip(note_carets(&doc, control)) {
            close(
                new["x"].as_f64().unwrap(),
                old["x"].as_f64().unwrap(),
                "미주 캐럿 x",
            );
            caret_delta(
                new["y"].as_f64().unwrap() - old["y"].as_f64().unwrap(),
                delta,
            );
            assert_eq!(new["height"], old["height"]);
            assert_eq!(new["pageIndex"], old["pageIndex"]);
        }
        assert_eq!(body_interaction(&doc), body, "본문 캐럿·선택 유지");
        assert_eq!(model_content(&doc, control), original);
        assert_eq!(doc.page_count(), 1);
    }
}

#[test]
fn double_endnote_separator_history_recovers_existing_single_dash_and_hidden_geometry() {
    for kind in [0, 1, 2] {
        let (mut doc, control) = fixture(5);
        doc.apply_endnote_shape_native(0, &json!({"separatorLineType":kind}).to_string())
            .unwrap();
        let before = doc.save_snapshot_native();
        let original = model_content(&doc, control);
        let before_geometry = geometry(&doc);
        let before_body = body_interaction(&doc);
        let before_note = note_carets(&doc, control);
        doc.apply_endnote_shape_native(0, r#"{"separatorLineType":8}"#)
            .unwrap();
        assert_double(&doc, 5);
        let after = doc.save_snapshot_native();
        let after_geometry = geometry(&doc);
        doc.restore_snapshot_native(before).unwrap();
        assert_eq!(
            value(&doc.get_endnote_shape_native(0).unwrap())["separatorLineType"],
            kind
        );
        assert_eq!(geometry(&doc), before_geometry, "기존 {kind} 배치 복원");
        assert_eq!(body_interaction(&doc), before_body);
        assert_eq!(note_carets(&doc, control), before_note);
        doc.restore_snapshot_native(after).unwrap();
        assert_double(&doc, 5);
        assert_eq!(geometry(&doc), after_geometry);
        assert_eq!(model_content(&doc, control), original);
    }
    let (mut doc, control) = fixture(5);
    let original = model_content(&doc, control);
    doc.apply_endnote_shape_native(0, r#"{"separatorLineType":8,"separatorLineWidth":0}"#)
        .unwrap();
    assert_double(&doc, 0);
    assert_eq!(model_content(&doc, control), original);
}

#[test]
fn double_endnote_separator_height_is_included_in_the_page_budget() {
    for width in [0, 1] {
        let (mut doc, control) = fixture(width);
        // glyph 높이와 줄 전진 높이를 같게 하여 기존 160% 줄간격의 다음 줄 예약과
        // 이중선 때문에 필요한 높이를 독립적으로 구분한다.
        doc.apply_para_format_native(0, 0, r#"{"lineSpacing":100}"#)
            .unwrap();
        doc.apply_para_format_in_footnote_native(0, 0, control, 0, r#"{"lineSpacing":100}"#)
            .unwrap();
        let original = model_content(&doc, control);
        let note = first_line(&doc, NOTE);
        // 단일선의 실제 마지막 줄 바로 아래에 본문 끝을 둔다. 이중선의 추가 높이는
        // 이 쪽에 들어갈 수 없으므로 다음 쪽으로 보내야 하며, 용지 아래로 그리면 안 된다.
        let height_hu = ((note.y + note.height + 10.0) * 75.0).ceil() as u32;
        doc.set_page_def_native(0, &json!({"height":height_hu}).to_string())
            .unwrap();
        assert_eq!(doc.page_count(), 1, "단일선과 한 줄 미주는 한 쪽에 맞는다");
        let before = first_line(&doc, NOTE);
        assert!(before.y + before.height <= f64::from(height_hu) / 75.0 - 10.0 + 0.002);
        doc.apply_endnote_shape_native(0, r#"{"separatorLineType":8}"#)
            .unwrap();
        assert_eq!(doc.page_count(), 2, "추가 잉크 높이도 페이지 예산에 포함");
        assert_eq!(model_content(&doc, control), original);
        for page in 0..doc.page_count() {
            fn check(node: &RenderNode, bottom: f64) {
                if let RenderNodeType::TextLine(_) = &node.node_type {
                    assert!(
                        node.bbox.y + node.bbox.height <= bottom + 0.002,
                        "실제 줄이 본문 아래로 넘지 않는다: {:?}",
                        node.bbox
                    );
                }
                for child in &node.children {
                    check(child, bottom);
                }
            }
            check(
                &doc.build_page_render_tree(page).unwrap().root,
                f64::from(height_hu) / 75.0 - 10.0,
            );
        }
    }
}

#[test]
fn double_endnote_separator_roundtrips_both_formats_without_mutating_content_or_source() {
    let (mut doc, control) = fixture(5);
    let single_hwp = doc.export_hwp_with_adapter_snapshot().unwrap();
    let single_hwpx = doc.export_hwpx_native().unwrap();
    doc.apply_endnote_shape_native(0, r#"{"separatorLineType":8}"#)
        .unwrap();
    let live = format!("{:?}", doc.document());
    let double_hwp = doc.export_hwp_with_adapter_snapshot().unwrap();
    let double_hwpx = doc.export_hwpx_native().unwrap();
    assert_eq!(
        format!("{:?}", doc.document()),
        live,
        "내보내기는 저장 사본만 수정"
    );
    for (format, single, double) in [
        ("HWP", single_hwp, double_hwp),
        ("HWPX", single_hwpx, double_hwpx),
    ] {
        let baseline = HwpDocument::from_bytes(&single).unwrap();
        let reopened = HwpDocument::from_bytes(&double).unwrap();
        let props = value(&reopened.get_endnote_shape_native(0).unwrap());
        assert_eq!(props["separatorLineType"], 8, "{format}");
        assert_eq!(props["separatorLineWidth"], 5);
        assert_eq!(props["separatorColor"], CSS_COLOR);
        assert_eq!(props["separatorMarginTop"], 0);
        assert_eq!(props["separatorMarginBottom"], 0);
        assert_double(&reopened, 5);
        assert_eq!(
            model_content(&reopened, control),
            model_content(&baseline, control),
            "{format}: 내용·번호·서식"
        );
        assert_eq!(
            body_interaction(&reopened),
            body_interaction(&baseline),
            "{format}: 본문 캐럿·선택"
        );
        let old_line = first_line(&baseline, NOTE);
        let new_line = first_line(&reopened, NOTE);
        let delta = new_line.y - old_line.y;
        for (old, new) in note_carets(&baseline, control)
            .iter()
            .zip(note_carets(&reopened, control))
        {
            close(
                new["x"].as_f64().unwrap(),
                old["x"].as_f64().unwrap(),
                "저장 미주 캐럿 x",
            );
            caret_delta(
                new["y"].as_f64().unwrap() - old["y"].as_f64().unwrap(),
                delta,
            );
            assert_eq!(new["height"], old["height"]);
            assert_eq!(new["pageIndex"], old["pageIndex"]);
        }
    }
}

fn assert_thin(doc: &HwpDocument, kind: u8) {
    if kind == 8 {
        assert_double(doc, 0);
        return;
    }
    let rendered = lines(doc);
    assert_eq!(rendered.len(), 1, "굵기0은 가시 단일선/파선");
    let line = &rendered[0];
    assert_eq!(line.style.line_type, LineRenderType::Single);
    assert_eq!(line.style.color, COLOR);
    assert_eq!(
        line.style.dash,
        if kind == 2 {
            StrokeDash::Dash
        } else {
            StrokeDash::Solid
        }
    );
    close(line.style.width, 0.32, "규격0.1mm의 600dpi 양자화 획");
    close(line.ink_bbox().height, 0.32, "실제 잉크 높이");
    close(line.x2 - line.x1, 80.0, "구분선 길이 유지");
    let ink = line.ink_bbox();
    assert!(
        first_line(doc, NOTE).y + 0.002 >= ink.y + ink.height,
        "0여백에서도 미주와 겹치지 않는다"
    );
    for profile in [RenderProfile::Screen, RenderProfile::Print] {
        let svg = doc
            .render_page_svg_layer_with_profile_native(0, profile)
            .unwrap();
        let tags = svg
            .split('<')
            .filter(|tag| tag.contains(&format!("stroke=\"{CSS_COLOR}\"")))
            .collect::<Vec<_>>();
        assert_eq!(tags.len(), 1);
        close(attribute(tags[0], "stroke-width"), 0.32, "SVG 실제 획 굵기");
        assert_eq!(tags[0].contains("stroke-dasharray=\"6 3\""), kind == 2);
    }
}

#[test]
fn thin_endnote_single_and_dash_preserve_geometry_content_and_edit_coordinates() {
    let (mut doc, control) = fixture(0);
    assert_thin(&doc, 1);
    let original = model_content(&doc, control);
    let single_geometry = geometry(&doc);
    let body = body_interaction(&doc);
    let note = note_carets(&doc, control);
    doc.apply_endnote_shape_native(0, r#"{"separatorLineType":2}"#)
        .unwrap();
    assert_thin(&doc, 2);
    assert_eq!(model_content(&doc, control), original);
    assert_eq!(body_interaction(&doc), body);
    assert_eq!(note_carets(&doc, control), note);
    doc.apply_endnote_shape_native(0, r#"{"separatorLineType":1}"#)
        .unwrap();
    assert_eq!(
        geometry(&doc),
        single_geometry,
        "종류 왕복은 기존 얇은 선 좌표를 복원"
    );
    assert_eq!(
        value(&doc.get_endnote_shape_native(0).unwrap())["separatorLineWidth"],
        0
    );
}

#[test]
fn thin_endnote_width_and_all_three_line_kinds_survive_history_and_both_formats() {
    for kind in [1, 2, 8] {
        let (mut doc, control) = fixture(0);
        let original = model_content(&doc, control);
        let before = doc.save_snapshot_native();
        let before_geometry = geometry(&doc);
        let baseline_hwp = doc.export_hwp_with_adapter_snapshot().unwrap();
        let baseline_hwpx = doc.export_hwpx_native().unwrap();
        doc.apply_endnote_shape_native(0, &json!({"separatorLineType":kind}).to_string())
            .unwrap();
        assert_thin(&doc, kind);
        let after = doc.save_snapshot_native();
        let after_geometry = geometry(&doc);
        let after_body = body_interaction(&doc);
        let after_note = note_carets(&doc, control);
        doc.restore_snapshot_native(before).unwrap();
        assert_thin(&doc, 1);
        assert_eq!(geometry(&doc), before_geometry);
        doc.restore_snapshot_native(after).unwrap();
        assert_thin(&doc, kind);
        assert_eq!(geometry(&doc), after_geometry);
        assert_eq!(body_interaction(&doc), after_body);
        assert_eq!(note_carets(&doc, control), after_note);
        assert_eq!(model_content(&doc, control), original);
        let live = format!("{:?}", doc.document());
        let saved_hwp = doc.export_hwp_with_adapter_snapshot().unwrap();
        let saved_hwpx = doc.export_hwpx_native().unwrap();
        assert_eq!(format!("{:?}", doc.document()), live, "저장은 사본만 변경");
        for (format, baseline, saved) in [
            ("HWP", baseline_hwp, saved_hwp),
            ("HWPX", baseline_hwpx, saved_hwpx),
        ] {
            let baseline = HwpDocument::from_bytes(&baseline).unwrap();
            let reopened = HwpDocument::from_bytes(&saved).unwrap();
            let props = value(&reopened.get_endnote_shape_native(0).unwrap());
            assert_eq!(props["separatorLineWidth"], 0, "{format}: 0.1mm raw 코드");
            assert_eq!(props["separatorLineType"], kind);
            assert_eq!(props["separatorEnabled"], true);
            assert_thin(&reopened, kind);
            assert_eq!(
                model_content(&reopened, control),
                model_content(&baseline, control)
            );
            assert_eq!(body_interaction(&reopened), body_interaction(&baseline));
            let delta = first_line(&reopened, NOTE).y - first_line(&baseline, NOTE).y;
            for (old, new) in note_carets(&baseline, control)
                .iter()
                .zip(note_carets(&reopened, control))
            {
                close(
                    new["x"].as_f64().unwrap(),
                    old["x"].as_f64().unwrap(),
                    "저장 후 미주 x",
                );
                caret_delta(
                    new["y"].as_f64().unwrap() - old["y"].as_f64().unwrap(),
                    delta,
                );
                assert_eq!(new["height"], old["height"]);
                assert_eq!(new["pageIndex"], old["pageIndex"]);
            }
        }
    }
}
