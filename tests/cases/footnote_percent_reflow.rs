//! 각주 백분율 간격은 첫 적용부터 현재 문단 모양으로 재조판한다.

use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use serde_json::Value;

fn json(text: String) -> Value {
    serde_json::from_str(&text).unwrap()
}

fn fixture() -> (DocumentCore, usize) {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    let body = "본문 앞🦦뒤 원문";
    core.insert_text_native(0, 0, 0, body).unwrap();
    core.apply_char_format_native(0, 0, 0, 2, r#"{"bold":true}"#)
        .unwrap();
    core.split_paragraph_native(0, 0, body.chars().count(), None)
        .unwrap();
    core.insert_text_native(0, 1, 0, "이웃 본문 문단 원문 보존")
        .unwrap();
    let made = json(core.insert_footnote_native(0, 0, 2).unwrap());
    let ci = made["controlIdx"].as_u64().unwrap() as usize;
    let text = "각주 줄 간격 한글🦦 원문과 글자 서식을 보존합니다. ".repeat(4);
    core.insert_text_in_footnote_native(0, 0, ci, 0, 2, &text)
        .unwrap();
    core.split_paragraph_in_footnote_native(0, 0, ci, 0, 2 + text.chars().count(), None)
        .unwrap();
    core.insert_text_in_footnote_native(0, 0, ci, 1, 0, "이웃 각주 문단 원문 보존")
        .unwrap();
    core.insert_click_here_field_at(0, 0, 5, "안내", "원래 메모", "이웃 필드", true)
        .unwrap();
    (core, ci)
}

fn export(core: &DocumentCore, hwpx: bool) -> Vec<u8> {
    if hwpx {
        core.export_hwpx_native().unwrap()
    } else {
        core.export_hwp_native().unwrap()
    }
}

fn note(core: &DocumentCore, ci: usize) -> &rhwp::model::footnote::Footnote {
    match &core.document().sections[0].paragraphs[0].controls[ci] {
        Control::Footnote(note) => note,
        _ => panic!("각주 주소가 바뀌었다"),
    }
}

#[derive(Debug, PartialEq)]
struct Scene {
    paragraphs: Value,
    trees: Vec<Value>,
    svg: Vec<String>,
    body_carets: Vec<Vec<Value>>,
    note_carets: Vec<Vec<Value>>,
}

fn scene(core: &DocumentCore, ci: usize) -> Scene {
    Scene {
        paragraphs: serde_json::to_value(&core.document().sections[0].paragraphs).unwrap(),
        trees: (0..core.page_count())
            .map(|page| json(core.build_page_render_tree(page).unwrap().root.to_json()))
            .collect(),
        svg: (0..core.page_count())
            .map(|page| core.render_page_svg_native(page).unwrap())
            .collect(),
        body_carets: core.document().sections[0]
            .paragraphs
            .iter()
            .enumerate()
            .map(|(pi, paragraph)| {
                (0..=paragraph.text.chars().count())
                    .map(|offset| json(core.get_cursor_rect_native(0, pi, offset).unwrap()))
                    .collect()
            })
            .collect(),
        note_carets: note(core, ci)
            .paragraphs
            .iter()
            .enumerate()
            .map(|(pi, paragraph)| {
                (0..=paragraph.text.chars().count())
                    .map(|offset| {
                        json(
                            core.get_cursor_rect_in_note_native(0, 0, ci, pi, offset)
                                .unwrap(),
                        )
                    })
                    .collect()
            })
            .collect(),
    }
}

fn collect<'a>(node: &'a Value, kind: &str, output: &mut Vec<&'a Value>) {
    if node["type"] == kind {
        output.push(node);
    }
    if let Some(children) = node["children"].as_array() {
        for child in children {
            collect(child, kind, output);
        }
    }
}

fn lines(scene: &Scene) -> Vec<&Value> {
    let mut areas = Vec::new();
    let mut lines = Vec::new();
    for tree in &scene.trees {
        collect(tree, "FootnoteArea", &mut areas);
    }
    for area in areas {
        collect(area, "TextLine", &mut lines);
    }
    lines
}

fn assert_preserved(before: &DocumentCore, after: &DocumentCore, ci: usize) {
    // 변경 대상의 문단 모양과 재조판 줄만 제외하고 원시 컨트롤·글자 위치·서식을 대조한다.
    let mut paragraphs = after.document().sections[0].paragraphs.clone();
    let Control::Footnote(changed) = &mut paragraphs[0].controls[ci] else {
        panic!("각주 소실");
    };
    changed.paragraphs[0].para_shape_id = note(before, ci).paragraphs[0].para_shape_id;
    changed.paragraphs[0].line_segs = note(before, ci).paragraphs[0].line_segs.clone();
    assert_eq!(
        serde_json::to_value(paragraphs).unwrap(),
        serde_json::to_value(&before.document().sections[0].paragraphs).unwrap(),
        "본문·필드·각주 번호·원문·문자별 서식·이웃 문단을 보존한다"
    );
    let before_scene = scene(before, ci);
    let after_scene = scene(after, ci);
    assert_eq!(after_scene.body_carets, before_scene.body_carets);
    let mut old_body = Vec::new();
    let mut new_body = Vec::new();
    for tree in &before_scene.trees {
        collect(tree, "Body", &mut old_body);
    }
    for tree in &after_scene.trees {
        collect(tree, "Body", &mut new_body);
    }
    assert_eq!(
        new_body, old_body,
        "이웃 본문 배치와 필드 안내문을 보존한다"
    );
}

fn check_format(hwpx: bool) {
    let (generated, ci) = fixture();
    let source = export(&generated, hwpx);
    let mut core = DocumentCore::from_bytes(&source).unwrap();
    assert_eq!(
        core.page_count(),
        generated.page_count(),
        "저장 전후 쪽 소속"
    );
    let before_bytes = export(&core, hwpx);
    let before = scene(&core, ci);
    let unchanged = DocumentCore::from_bytes(&before_bytes).unwrap();
    let original = core.document().clone();
    let original_core = DocumentCore::from_bytes(&source).unwrap();
    let undo = core.save_snapshot_native();
    let old_lines = lines(&before);
    assert!(old_lines.len() >= 4);
    let old_gap =
        old_lines[1]["bbox"]["y"].as_f64().unwrap() - old_lines[0]["bbox"]["y"].as_f64().unwrap();
    let old_props = json(
        core.get_para_properties_in_footnote_native(0, 0, ci, 0)
            .unwrap(),
    );
    assert_eq!(old_props["lineSpacing"].as_f64(), Some(160.0));
    core.apply_para_format_in_footnote_native(
        0,
        0,
        ci,
        0,
        r#"{"lineSpacingType":"Percent","lineSpacing":240}"#,
    )
    .unwrap();
    let after = scene(&core, ci);
    let new_lines = lines(&after);
    let new_gap =
        new_lines[1]["bbox"]["y"].as_f64().unwrap() - new_lines[0]["bbox"]["y"].as_f64().unwrap();
    // 백분율의 독립 계약: 같은 글자·줄 구성에서는 줄 전진이 240/160 배가 된다.
    assert!(
        (new_gap - old_gap * 1.5).abs() < 0.2,
        "첫 적용 줄 전진: {old_gap} → {new_gap}"
    );
    assert_eq!(new_lines.len(), old_lines.len());
    assert_ne!(after.note_carets, before.note_carets);
    assert_preserved(&original_core, &core, ci);
    let mut props = json(
        core.get_para_properties_in_footnote_native(0, 0, ci, 0)
            .unwrap(),
    );
    props["paraShapeId"] = old_props["paraShapeId"].clone();
    let mut expected_props = old_props;
    expected_props["lineSpacing"] = 240.0.into();
    assert_eq!(props, expected_props);
    let after_bytes = export(&core, hwpx);
    let reopened = DocumentCore::from_bytes(&after_bytes).unwrap();
    assert_preserved(&unchanged, &reopened, ci);
    assert_eq!(
        scene(&reopened, ci),
        after,
        "같은 형식 재열기 배치·캐럿·원문"
    );
    let redo = core.save_snapshot_native();
    core.restore_snapshot_native(undo).unwrap();
    assert_eq!(scene(&core, ci), before);
    assert_eq!(export(&core, hwpx), before_bytes);
    assert_eq!(
        serde_json::to_value(&core.document().sections[0].paragraphs).unwrap(),
        serde_json::to_value(&original.sections[0].paragraphs).unwrap()
    );
    core.restore_snapshot_native(redo).unwrap();
    assert_eq!(scene(&core, ci), after);
    assert_eq!(export(&core, hwpx), after_bytes);
    core.discard_snapshot_native(undo);
    core.discard_snapshot_native(redo);
}

#[test]
fn hwp_percent_first_apply_save_and_snapshot_preserve_neighbors() {
    check_format(false);
}

#[test]
fn hwpx_percent_first_apply_save_and_snapshot_preserve_neighbors() {
    check_format(true);
}
