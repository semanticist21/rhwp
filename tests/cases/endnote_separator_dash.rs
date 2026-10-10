//! 미주 파선은 저장된 종류 2를 공통 파선으로 그리며 미주 배치와 편집 좌표를 바꾸지 않는다.
//! 이중선·번호 이어 매기기·구역별 배치와 한컴 인쇄의 시각 일치는 이 검사의 범위가 아니다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::{
    model::control::Control,
    paint::RenderProfile,
    renderer::render_tree::{LineNode, RenderNode, RenderNodeType},
    renderer::StrokeDash,
    wasm_api::HwpDocument,
};
use serde_json::{json, Value};

const BODY: &str = "본문 앞과 뒤를 보존합니다";
const NOTE: &str = "미주 내용과 캐럿을 보존합니다";
const COLOR: u32 = 0x5030d0;
const CSS_COLOR: &str = "#d03050";

fn fixture(width: u8) -> (HwpDocument, usize) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, BODY).unwrap();
    let inserted: Value = serde_json::from_str(
        &doc.insert_endnote_native(0, 0, BODY.chars().count())
            .unwrap(),
    )
    .unwrap();
    let control = inserted["controlIdx"].as_u64().unwrap() as usize;
    // 새 미주 문단의 번호와 간격 두 칸 뒤에 실제 내용을 넣는다.
    doc.insert_text_in_footnote_native(0, 0, control, 0, 2, NOTE)
        .unwrap();
    doc.apply_endnote_shape_native(
        0,
        &json!({
            "startNumber": 3, "prefixChar": "[", "suffixChar": "]",
            "separatorLineType": 1, "separatorLineWidth": width,
            "separatorLength": 6000, "separatorColor": CSS_COLOR,
            "separatorMarginTop": 300, "separatorMarginBottom": 450,
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(doc.page_count(), 1, "작은 본문과 미주 fixture");
    (doc, control)
}

fn shape(doc: &HwpDocument) -> Value {
    serde_json::from_str(&doc.get_endnote_shape_native(0).unwrap()).unwrap()
}

fn separators(doc: &HwpDocument) -> Vec<LineNode> {
    fn collect(node: &RenderNode, out: &mut Vec<LineNode>) {
        if let RenderNodeType::Line(line) = &node.node_type {
            if line.style.color == COLOR {
                out.push(line.clone());
            }
        }
        for child in &node.children {
            collect(child, out);
        }
    }
    let mut out = Vec::new();
    for page in 0..doc.page_count() {
        collect(&doc.build_page_render_tree(page).unwrap().root, &mut out);
    }
    out
}

fn geometry(node: &RenderNode) -> Value {
    let mut kind = node.node_type.clone();
    if let RenderNodeType::Line(line) = &mut kind {
        if line.style.color == COLOR {
            line.style.dash = StrokeDash::Solid;
        }
    }
    json!({
        "bbox": node.bbox, "kind": kind,
        "children": node.children.iter().map(geometry).collect::<Vec<_>>(),
    })
}

fn layout(doc: &HwpDocument) -> Value {
    json!((0..doc.page_count())
        .map(|page| geometry(&doc.build_page_render_tree(page).unwrap().root))
        .collect::<Vec<_>>())
}

fn content(doc: &HwpDocument, control: usize) -> Value {
    let para = &doc.document().sections[0].paragraphs[0];
    let Control::Endnote(note) = &para.controls[control] else {
        panic!("미주 컨트롤 유지");
    };
    assert_eq!(para.text.matches(BODY).count(), 1, "본문 원문 한 번");
    assert_eq!(note.paragraphs.len(), 1);
    assert_eq!(
        note.paragraphs[0].text.matches(NOTE).count(),
        1,
        "미주 원문 한 번"
    );
    assert_eq!(note.number, 3);
    json!({
        "body": para.text, "bodyShapes": para.char_shapes,
        "note": note.paragraphs[0].text, "noteShapes": note.paragraphs[0].char_shapes,
        "number": note.number, "before": note.before_decoration_letter,
        "after": note.after_decoration_letter, "numberShape": note.number_shape,
    })
}

fn interaction(doc: &HwpDocument, control: usize) -> Value {
    let body_carets = (0..=BODY.chars().count())
        .map(|offset| doc.get_cursor_rect_native(0, 0, offset).unwrap())
        .collect::<Vec<_>>();
    let note_carets = (0..=NOTE.chars().count())
        .map(|offset| {
            doc.get_cursor_rect_in_note_native(0, 0, control, 0, offset + 2)
                .unwrap()
        })
        .collect::<Vec<_>>();
    let selection: Value =
        serde_json::from_str(&doc.get_selection_rects(0, 0, 1, 0, 6).unwrap()).unwrap();
    assert!(!selection.as_array().unwrap().is_empty(), "실제 본문 선택");
    json!({"body": body_carets, "note": note_carets, "selection": selection, "pages": doc.page_count()})
}

fn assert_dash(doc: &HwpDocument, expected: bool) {
    let lines = separators(doc);
    assert_eq!(lines.len(), 1, "미주 구분선 한 개");
    assert_eq!(
        lines[0].style.dash,
        if expected {
            StrokeDash::Dash
        } else {
            StrokeDash::Solid
        }
    );
    assert_eq!(lines[0].y1, lines[0].y2, "기존 수평선 유지");
    assert!(lines[0].x2 > lines[0].x1);
    for profile in [RenderProfile::Screen, RenderProfile::Print] {
        let svg = doc
            .render_page_svg_layer_with_profile_native(0, profile)
            .unwrap();
        let tags = svg
            .split('<')
            .filter(|tag| tag.contains(&format!("stroke=\"{CSS_COLOR}\"")))
            .collect::<Vec<_>>();
        assert_eq!(tags.len(), 1, "{profile:?}: 실제 미주 선");
        if expected {
            assert!(
                tags[0].contains("stroke-dasharray=\"6 3\""),
                "{profile:?}: {tags:?}"
            );
        } else {
            assert!(
                !tags[0].contains("stroke-dasharray"),
                "{profile:?}: 기존 실선"
            );
        }
    }
}

#[test]
fn dashed_endnote_separator_preserves_all_text_layout_carets_and_selection() {
    // 얇은 선·기본선·굵은 선에서도 파선 연결이 예약 높이나 획 굵기를 바꾸지 않는다.
    for width in [1, 5, 15] {
        let (mut doc, control) = fixture(width);
        assert_dash(&doc, false);
        let before_layout = layout(&doc);
        let before_interaction = interaction(&doc, control);
        let before_content = content(&doc, control);
        doc.apply_endnote_shape_native(0, r#"{"separatorLineType":2}"#)
            .unwrap();
        assert_eq!(shape(&doc)["separatorLineType"], 2);
        assert_eq!(shape(&doc)["separatorLineWidth"], width);
        assert_dash(&doc, true);
        assert_eq!(layout(&doc), before_layout, "파선만 바뀌고 전체 배치 유지");
        assert_eq!(interaction(&doc, control), before_interaction);
        assert_eq!(content(&doc, control), before_content);
    }
}

#[test]
fn dashed_endnote_separator_snapshot_restore_recovers_solid_and_dash() {
    let (mut doc, control) = fixture(5);
    let before = doc.save_snapshot_native();
    let original = content(&doc, control);
    let original_layout = layout(&doc);
    let original_interaction = interaction(&doc, control);
    doc.apply_endnote_shape_native(0, r#"{"separatorLineType":2}"#)
        .unwrap();
    let after = doc.save_snapshot_native();
    assert_dash(&doc, true);
    doc.restore_snapshot_native(before).unwrap();
    assert_eq!(shape(&doc)["separatorLineType"], 1);
    assert_dash(&doc, false);
    doc.restore_snapshot_native(after).unwrap();
    assert_eq!(shape(&doc)["separatorLineType"], 2);
    assert_dash(&doc, true);
    assert_eq!(content(&doc, control), original);
    assert_eq!(layout(&doc), original_layout);
    assert_eq!(interaction(&doc, control), original_interaction);
}

#[test]
fn dashed_endnote_separator_roundtrips_both_formats_without_mutating_source() {
    let (mut doc, control) = fixture(5);
    let solid_hwp = doc.export_hwp_with_adapter_snapshot().unwrap();
    let solid_hwpx = doc.export_hwpx_native().unwrap();
    doc.apply_endnote_shape_native(0, r#"{"separatorLineType":2}"#)
        .unwrap();
    let live_model = format!("{:?}", doc.document());
    let dashed_hwp = doc.export_hwp_with_adapter_snapshot().unwrap();
    let dashed_hwpx = doc.export_hwpx_native().unwrap();
    assert_eq!(
        format!("{:?}", doc.document()),
        live_model,
        "저장 사본만 사용"
    );
    for (name, solid, dashed) in [
        ("HWP", solid_hwp, dashed_hwp),
        ("HWPX", solid_hwpx, dashed_hwpx),
    ] {
        let baseline = HwpDocument::from_bytes(&solid).unwrap();
        let reopened = HwpDocument::from_bytes(&dashed).unwrap();
        assert_eq!(shape(&reopened)["separatorLineType"], 2, "{name}");
        assert_eq!(shape(&reopened)["separatorLineWidth"], 5, "{name}");
        assert_eq!(shape(&reopened)["separatorColor"], CSS_COLOR, "{name}");
        assert_dash(&baseline, false);
        assert_dash(&reopened, true);
        assert_eq!(
            content(&reopened, control),
            content(&baseline, control),
            "{name}: 내용·번호·모양"
        );
        assert_eq!(layout(&reopened), layout(&baseline), "{name}: 형식별 배치");
        assert_eq!(
            interaction(&reopened, control),
            interaction(&baseline, control),
            "{name}: 형식별 편집 좌표"
        );
    }
}

#[test]
fn disabled_and_zero_width_endnote_separators_remain_absent() {
    for props in [
        r#"{"separatorEnabled":false}"#,
        r#"{"separatorLineWidth":0}"#,
        r#"{"separatorLineType":0}"#,
    ] {
        let (mut doc, control) = fixture(5);
        let original = content(&doc, control);
        doc.apply_endnote_shape_native(0, r#"{"separatorLineType":2}"#)
            .unwrap();
        doc.apply_endnote_shape_native(0, props).unwrap();
        assert!(separators(&doc).is_empty(), "{props}: 기존 숨김 조건");
        assert_eq!(content(&doc, control), original);
        for profile in [RenderProfile::Screen, RenderProfile::Print] {
            assert!(!doc
                .render_page_svg_layer_with_profile_native(0, profile)
                .unwrap()
                .contains(&format!("stroke=\"{CSS_COLOR}\"")));
        }
    }
}
