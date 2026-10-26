//! 0.1mm 미주 구분선(실선·파선·이중선)은 편집 이력과 두 저장 형식에서 같은 모양을 지킨다.
//! 구분선 굵기는 흐름 높이에 더하지 않는다(devel의 한컴 출력 계약). 선은 위 여백 끝에
//! 긋고 첫 미주는 그 자리 + 아래 여백에서 시작한다.
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

fn assert_thin(doc: &HwpDocument, kind: u8) {
    let rendered = lines(doc);
    assert_eq!(rendered.len(), 1, "굵기0은 가시 단일선/파선/이중선");
    let line = &rendered[0];
    assert_eq!(
        line.style.line_type,
        if kind == 8 {
            LineRenderType::Double
        } else {
            LineRenderType::Single
        }
    );
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
    close(line.x2 - line.x1, 80.0, "구분선 길이 유지");
    // 굵기는 흐름에 더하지 않으므로 여백 0이면 첫 미주는 선 중심에서 시작한다.
    assert!(
        first_line(doc, NOTE).y + 0.002 >= line.y1,
        "0여백에서 첫 미주는 선 중심보다 위로 올라가지 않는다"
    );
    for profile in [RenderProfile::Screen, RenderProfile::Print] {
        let svg = doc
            .render_page_svg_layer_with_profile_native(0, profile)
            .unwrap();
        let tags = svg
            .split('<')
            .filter(|tag| tag.contains(&format!("stroke=\"{CSS_COLOR}\"")))
            .collect::<Vec<_>>();
        // 이중선은 SVG에서 두 획으로 그린다.
        assert_eq!(tags.len(), if kind == 8 { 2 } else { 1 });
        if kind != 8 {
            close(attribute(tags[0], "stroke-width"), 0.32, "SVG 실제 획 굵기");
        }
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
