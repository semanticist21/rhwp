//! 첫 미주 문단의 숨긴 선행 공백은 원문 주소를 유지하며 보이는 글자 경계로 변환한다.
//! 여러 쪽 주석·미주 hit-test·선택 구현은 이 검사의 범위가 아니다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::{
    model::control::Control,
    renderer::render_tree::{RenderNode, RenderNodeType},
    wasm_api::HwpDocument,
};
use serde_json::{json, Value};

const BODY: &str = "앞 본문과 뒤 본문";
const FIRST: &str = "첫째🦦 끝";
const SECOND: &str = "  둘째🦦 문단";

fn fixture(extra_spaces: &str, decorated: bool, second: bool) -> (HwpDocument, usize) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, BODY).unwrap();
    let inserted: Value =
        serde_json::from_str(&doc.insert_endnote_native(0, 0, 2).unwrap()).unwrap();
    let control = inserted["controlIdx"].as_u64().unwrap() as usize;
    if second {
        // 분할 API의 논리 주소는 두 선행 공백과 번호 개체 한 칸을 함께 센다.
        doc.split_paragraph_in_footnote_native(0, 0, control, 0, 3, None)
            .unwrap();
        doc.insert_text_in_footnote_native(0, 0, control, 1, 0, SECOND)
            .unwrap();
    }
    doc.insert_text_in_footnote_native(0, 0, control, 0, 2, &format!("{extra_spaces}{FIRST}"))
        .unwrap();
    if decorated {
        doc.apply_endnote_shape_native(
            0,
            r#"{"startNumber":12,"prefixChar":"[","suffixChar":"]"}"#,
        )
        .unwrap();
    }
    assert_eq!(doc.page_count(), 1, "짧은 미주는 한 쪽 안에 둔다");
    (doc, control)
}

fn note_texts(doc: &HwpDocument, control: usize) -> Vec<String> {
    let Control::Endnote(note) = &doc.document().sections[0].paragraphs[0].controls[control] else {
        panic!("원래 미주 컨트롤");
    };
    note.paragraphs.iter().map(|p| p.text.clone()).collect()
}

fn geometry(node: &RenderNode) -> Value {
    json!({
        "bbox": node.bbox, "kind": node.node_type,
        "children": node.children.iter().map(geometry).collect::<Vec<_>>(),
    })
}

fn rect(doc: &HwpDocument, control: usize, paragraph: usize, offset: usize) -> Value {
    serde_json::from_str(
        &doc.get_cursor_rect_in_note_native(0, 0, control, paragraph, offset)
            .unwrap(),
    )
    .unwrap()
}

fn rendered_cursor_y(node: &RenderNode, para: usize, start: usize, text: &str) -> Option<f64> {
    if let RenderNodeType::TextRun(run) = &node.node_type {
        if run.section_index == Some(0)
            && run.para_index == Some(para)
            && run.char_start == Some(start)
            && run.text == text
        {
            return Some(node.bbox.y + run.baseline - run.style.font_size * 0.8);
        }
    }
    node.children
        .iter()
        .find_map(|child| rendered_cursor_y(child, para, start, text))
}

fn assert_visible_boundaries(
    doc: &HwpDocument,
    control: usize,
    paragraph: usize,
    hidden: usize,
    prefix: &str,
    visible: &str,
) {
    let info: Value =
        serde_json::from_str(&doc.get_note_edit_info_native(0, 0, control).unwrap()).unwrap();
    assert_eq!(info["kind"], "endnote");
    assert_eq!(info["charOffset"], 2, "편집 진입의 원문 주소 유지");
    let virtual_para = info["virtualParaIndex"].as_u64().unwrap() as usize + paragraph;
    let tree = doc.build_page_render_tree(0).unwrap();
    let layout: Value = serde_json::from_str(&doc.get_page_text_layout_native(0).unwrap()).unwrap();
    let mut runs = layout["runs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|run| run["secIdx"] == 0 && run["paraIdx"] == virtual_para)
        .collect::<Vec<_>>();
    runs.sort_by_key(|run| run["charStart"].as_u64().unwrap());
    assert_eq!(
        runs.iter()
            .map(|run| run["text"].as_str().unwrap())
            .collect::<String>(),
        format!("{prefix}{visible}"),
        "독립 기대: 렌더 사본만 번호와 선행 공백을 변환한다",
    );
    let mut xs = Vec::new();
    for offset in 0..=visible.chars().count() {
        let rendered_offset = prefix.chars().count() + offset;
        let run = runs
            .iter()
            .find(|run| {
                let start = run["charStart"].as_u64().unwrap() as usize;
                let end = start + run["text"].as_str().unwrap().chars().count();
                rendered_offset >= start && rendered_offset <= end
            })
            .unwrap();
        let local = rendered_offset - run["charStart"].as_u64().unwrap() as usize;
        let expected_x = run["x"].as_f64().unwrap() + run["charX"][local].as_f64().unwrap();
        let caret = rect(doc, control, paragraph, hidden + offset);
        let x = caret["x"].as_f64().unwrap();
        // 공개 layout의 x와 charX는 각각 0.1px로 직렬화되므로 합의 양자화 오차만 허용한다.
        assert!(
            (x - expected_x).abs() <= 0.100_001,
            "문단 {paragraph} 원문 offset {}: 캐럿 {x}, 실제 TextRun 경계 {expected_x}",
            hidden + offset,
        );
        assert_eq!(caret["pageIndex"], 0);
        let expected_y = rendered_cursor_y(
            &tree.root,
            virtual_para,
            run["charStart"].as_u64().unwrap() as usize,
            run["text"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(
            caret["y"].as_f64().unwrap(),
            format!("{expected_y:.1}").parse::<f64>().unwrap(),
            "실제 run의 baseline 캐럿 위치",
        );
        assert_eq!(caret["height"], run["fontSize"], "기존 글꼴 캐럿 높이");
        xs.push(x);
    }
    assert!(
        xs.windows(2).all(|pair| pair[1] > pair[0]),
        "가시 문자 앞뒤가 겹치면 안 된다: {xs:?}"
    );
    if paragraph == 0 {
        let first = rect(doc, control, 0, hidden);
        for offset in 0..hidden {
            assert_eq!(
                rect(doc, control, 0, offset),
                first,
                "숨긴 공백은 번호 뒤 한 위치"
            );
        }
    }
}

#[test]
fn endnote_first_paragraph_carets_match_rendered_scalar_boundaries() {
    for (extra, decorated, prefix) in [("", false, "1) "), ("\u{00a0}\u{2007} ", true, "[12] ")] {
        let (doc, control) = fixture(extra, decorated, false);
        let expected = format!("  {extra}{FIRST}");
        assert_eq!(
            note_texts(&doc, control).as_slice(),
            std::slice::from_ref(&expected)
        );
        assert!(
            expected.encode_utf16().count() > expected.chars().count(),
            "보조 평면 문자 fixture"
        );
        let model = format!("{:?}", doc.document());
        let layout = geometry(&doc.build_page_render_tree(0).unwrap().root);
        assert_visible_boundaries(&doc, control, 0, 2 + extra.chars().count(), prefix, FIRST);
        assert_eq!(
            format!("{:?}", doc.document()),
            model,
            "좌표 조회는 원문·서식·dirty를 바꾸지 않는다"
        );
        assert_eq!(
            geometry(&doc.build_page_render_tree(0).unwrap().root),
            layout
        );
    }
}

#[test]
fn endnote_second_paragraph_keeps_its_visible_leading_spaces() {
    let (doc, control) = fixture("", true, true);
    assert_eq!(
        note_texts(&doc, control),
        [format!("  {FIRST}"), SECOND.to_string()]
    );
    assert_visible_boundaries(&doc, control, 0, 2, "[12] ", FIRST);
    assert_visible_boundaries(&doc, control, 1, 0, "", SECOND);
}

#[test]
fn endnote_caret_queries_and_snapshot_restore_preserve_model_and_layout() {
    let (mut doc, control) = fixture("\u{00a0}\u{2007}", true, true);
    let before = doc.save_snapshot_native();
    let original = note_texts(&doc, control);
    let layout = geometry(&doc.build_page_render_tree(0).unwrap().root);
    assert_visible_boundaries(&doc, control, 0, 4, "[12] ", FIRST);
    doc.insert_text_in_footnote_native(0, 0, control, 0, 4, "추가")
        .unwrap();
    let after = doc.save_snapshot_native();
    assert_visible_boundaries(&doc, control, 0, 4, "[12] ", &format!("추가{FIRST}"));
    doc.restore_snapshot_native(before).unwrap();
    assert_eq!(note_texts(&doc, control), original);
    assert_eq!(
        geometry(&doc.build_page_render_tree(0).unwrap().root),
        layout
    );
    assert_visible_boundaries(&doc, control, 0, 4, "[12] ", FIRST);
    doc.restore_snapshot_native(after).unwrap();
    assert_eq!(
        note_texts(&doc, control)[0],
        format!("  \u{00a0}\u{2007}추가{FIRST}")
    );
    assert_visible_boundaries(&doc, control, 0, 4, "[12] ", &format!("추가{FIRST}"));
    assert_visible_boundaries(&doc, control, 1, 0, "", SECOND);
}

#[test]
fn endnote_caret_offsets_roundtrip_hwp_and_hwpx_without_live_mutation() {
    let (mut doc, control) = fixture("\u{00a0}\u{2007}", true, true);
    let original = note_texts(&doc, control);
    let live = format!("{:?}", doc.document());
    for (name, bytes) in [
        ("HWP", doc.export_hwp_with_adapter_snapshot().unwrap()),
        ("HWPX", doc.export_hwpx_native().unwrap()),
    ] {
        let reopened = HwpDocument::from_bytes(&bytes).unwrap();
        assert_eq!(
            note_texts(&reopened, control),
            original,
            "{name}: 원문 그대로"
        );
        assert_visible_boundaries(&reopened, control, 0, 4, "[12] ", FIRST);
        assert_visible_boundaries(&reopened, control, 1, 0, "", SECOND);
        let Control::Endnote(note) =
            &reopened.document().sections[0].paragraphs[0].controls[control]
        else {
            panic!("{name}: 미주 컨트롤");
        };
        assert_eq!(note.number, 12);
        assert_eq!(note.before_decoration_letter, '[' as u16);
        assert_eq!(note.after_decoration_letter, ']' as u16);
    }
    assert_eq!(format!("{:?}", doc.document()), live, "저장 사본만 변환");
}
