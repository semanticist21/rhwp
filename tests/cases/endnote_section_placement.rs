//! 미주의 문서 끝/구역 끝 전환은 현재 구역과 마지막 구역의 실제 배치를 함께 갱신한다.
//! 번호 이어 매기기·여러 쪽 미주·다른 구역에 미뤄진 미주의 편집 주소는 범위 밖이다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::{
    model::control::Control,
    renderer::render_tree::{RenderNode, RenderNodeType},
    wasm_api::HwpDocument,
};
use serde_json::{json, Value};

const BODIES: [&str; 2] = ["첫 구역의 고유 본문", "둘째 구역의 고유 본문"];
const NOTES: [&str; 2] = ["미주 A 고유 내용", "미주 B 고유 내용"];

struct Fixture {
    doc: HwpDocument,
    controls: [usize; 2],
}

fn fixture() -> Fixture {
    // 구역 나눔 명령의 문단 부호와 실제 sections 목록을 혼동하지 않는 두 구역 입력이다.
    let sections = BODIES
        .iter()
        .enumerate()
        .map(|(section, body)| {
            format!(
                r#"<SECTION Id="{section}"><P ParaShape="0" Style="0"><TEXT CharShape="0"><SECDEF><PAGEDEF Width="59528" Height="84188"><PAGEMARGIN Left="8504" Right="8504" Top="5669" Bottom="4252" Header="4252" Footer="4252" Gutter="0"/></PAGEDEF></SECDEF><CHAR>{body}</CHAR></TEXT></P></SECTION>"#,
            )
        })
        .collect::<String>();
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><HWPML Version="2.91"><HEAD SecCnt="2"><MAPPINGTABLE><CHARSHAPELIST Count="1"><CHARSHAPE Id="0" Height="1000"/></CHARSHAPELIST><PARASHAPELIST Count="1"><PARASHAPE Id="0" Align="Left"/></PARASHAPELIST><STYLELIST Count="1"><STYLE Id="0" Name="바탕글" Type="Para" ParaShape="0" CharShape="0" NextStyle="0"/></STYLELIST></MAPPINGTABLE></HEAD><BODY>{sections}</BODY></HWPML>"#,
    );
    let mut doc = HwpDocument::from_bytes(xml.as_bytes()).unwrap();
    assert_eq!(doc.document().sections.len(), 2);
    for section in 0..2 {
        let inserted: Value = serde_json::from_str(
            &doc.insert_endnote_native(section, 0, BODIES[section].chars().count())
                .unwrap(),
        )
        .unwrap();
        let control = inserted["controlIdx"].as_u64().unwrap() as usize;
        doc.insert_text_in_footnote_native(section, 0, control, 0, 2, NOTES[section])
            .unwrap();
        doc.apply_endnote_shape_native(
            section,
            r#"{"placement":"documentEnd","startNumber":1,"prefixChar":"","suffixChar":")"}"#,
        )
        .unwrap();
    }
    // 저장본을 새로 열어 두 구역의 최초 배치는 모두 새로 만든 상태에서 전환을 시작한다.
    let doc = HwpDocument::from_bytes(&doc.export_hwpx_native().unwrap()).unwrap();
    let controls = std::array::from_fn(|section| {
        doc.document().sections[section].paragraphs[0]
            .controls
            .iter()
            .position(|control| matches!(control, Control::Endnote(_)))
            .unwrap()
    });
    Fixture { doc, controls }
}

fn shape(doc: &HwpDocument, section: usize) -> Value {
    serde_json::from_str(&doc.get_endnote_shape_native(section).unwrap()).unwrap()
}

fn page_layouts(doc: &HwpDocument) -> Vec<Value> {
    (0..doc.page_count())
        .map(|page| serde_json::from_str(&doc.get_page_text_layout_native(page).unwrap()).unwrap())
        .collect()
}

fn body_caret(doc: &HwpDocument, section: usize, offset: usize) -> Value {
    serde_json::from_str(&doc.get_cursor_rect_native(section, 0, offset).unwrap()).unwrap()
}

fn protected_content(doc: &HwpDocument) -> Value {
    json!(doc
        .document()
        .sections
        .iter()
        .map(|section| {
            let para = &section.paragraphs[0];
            let notes = para
                .controls
                .iter()
                .filter_map(|control| match control {
                    Control::Endnote(note) => Some(note),
                    _ => None,
                })
                .collect::<Vec<_>>();
            json!({
                "body": para.text, "bodyShapes": para.char_shapes,
                "bodyOffsets": para.char_offsets, "bodyParaShape": para.para_shape_id,
                "bodyStyle": para.style_id, "notes": notes,
            })
        })
        .collect::<Vec<_>>())
}

fn body_interaction(doc: &HwpDocument) -> Value {
    json!((0..2)
        .map(|section| {
            let carets = (0..=BODIES[section].chars().count())
                .map(|offset| body_caret(doc, section, offset))
                .collect::<Vec<_>>();
            let selection: Value =
                serde_json::from_str(&doc.get_selection_rects(section as u32, 0, 2, 0, 6).unwrap())
                    .unwrap();
            let rects = selection.as_array().unwrap();
            assert!(!rects.is_empty(), "구역 {section}: 본문 범위 선택");
            assert!(rects.iter().all(|rect| {
                rect["width"].as_f64().unwrap() > 0.0 && rect["height"].as_f64().unwrap() > 0.0
            }));
            json!({"carets": carets, "selection": selection})
        })
        .collect::<Vec<_>>())
}

fn assert_placement(doc: &HwpDocument, first_placement: &str) {
    assert_eq!(doc.document().sections.len(), 2);
    assert_eq!(doc.page_count(), 2, "짧은 본문과 미주는 각 구역 한 쪽");
    assert_eq!(shape(doc, 0)["placement"], first_placement);
    assert_eq!(shape(doc, 1)["placement"], "documentEnd");
    let pages = page_layouts(doc)
        .iter()
        .map(|page| {
            page["runs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|run| run["text"].as_str().unwrap())
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    let body_pages = std::array::from_fn::<_, 2, _>(|section| {
        let page = body_caret(doc, section, 0)["pageIndex"].as_u64().unwrap() as usize;
        assert_eq!(pages[page].matches(BODIES[section]).count(), 1);
        assert_eq!(
            pages
                .iter()
                .map(|page| page.matches(BODIES[section]).count())
                .sum::<usize>(),
            1,
            "구역 {section}: 본문을 한 번만 그린다",
        );
        page
    });
    assert!(body_pages[0] < body_pages[1], "원래 구역 순서");
    for (section, expected_page) in [
        (
            0,
            body_pages[if first_placement == "sectionEnd" {
                0
            } else {
                1
            }],
        ),
        (1, body_pages[1]),
    ] {
        assert_eq!(
            pages
                .iter()
                .map(|page| page.matches(NOTES[section]).count())
                .sum::<usize>(),
            1,
            "미주 {section}: 캐시 잔재·중복·누락 없이 한 번만 그린다: {pages:?}",
        );
        assert_eq!(pages[expected_page].matches(NOTES[section]).count(), 1);
        let notes = doc.document().sections[section].paragraphs[0]
            .controls
            .iter()
            .filter_map(|control| match control {
                Control::Endnote(note) => Some(note),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(notes.len(), 1, "원본 미주 컨트롤 한 개");
        assert_eq!(notes[0].number, 1, "기존 번호 유지");
        assert_eq!(notes[0].paragraphs.len(), 1);
        assert_eq!(notes[0].paragraphs[0].text, format!("  {}", NOTES[section]));
    }
}

fn assert_note_carets(doc: &HwpDocument, section: usize, control: usize) {
    fn cursor_y(node: &RenderNode, section: usize, para: usize, start: usize) -> Option<f64> {
        if let RenderNodeType::TextRun(run) = &node.node_type {
            if run.section_index == Some(section)
                && run.para_index == Some(para)
                && run.char_start == Some(start)
            {
                return Some(node.bbox.y + run.baseline - run.style.font_size * 0.8);
            }
        }
        node.children
            .iter()
            .find_map(|child| cursor_y(child, section, para, start))
    }
    let info: Value =
        serde_json::from_str(&doc.get_note_edit_info_native(section, 0, control).unwrap()).unwrap();
    assert_eq!(info["kind"], "endnote");
    let page = info["pageNum"].as_u64().unwrap() as u32;
    let para = info["virtualParaIndex"].as_u64().unwrap();
    let layout: Value =
        serde_json::from_str(&doc.get_page_text_layout_native(page).unwrap()).unwrap();
    let tree = doc.build_page_render_tree(page).unwrap();
    let runs = layout["runs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|run| run["secIdx"] == section && run["paraIdx"] == para)
        .collect::<Vec<_>>();
    assert_eq!(
        runs.iter()
            .map(|run| run["text"].as_str().unwrap())
            .collect::<String>(),
        format!("1) {}", NOTES[section]),
    );
    for offset in 0..=NOTES[section].chars().count() {
        let rendered_offset = offset + 3;
        let run = runs
            .iter()
            .find(|run| {
                let start = run["charStart"].as_u64().unwrap() as usize;
                rendered_offset >= start
                    && rendered_offset <= start + run["text"].as_str().unwrap().chars().count()
            })
            .unwrap();
        let local = rendered_offset - run["charStart"].as_u64().unwrap() as usize;
        let expected_x = run["x"].as_f64().unwrap() + run["charX"][local].as_f64().unwrap();
        let caret: Value = serde_json::from_str(
            &doc.get_cursor_rect_in_note_native(section, 0, control, 0, offset + 2)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(caret["pageIndex"], page);
        // 공개 layout의 x와 charX는 각각 0.1px로 직렬화된다.
        assert!((caret["x"].as_f64().unwrap() - expected_x).abs() <= 0.100_001);
        let expected_y = cursor_y(
            &tree.root,
            section,
            para as usize,
            run["charStart"].as_u64().unwrap() as usize,
        )
        .unwrap();
        assert_eq!(
            caret["y"].as_f64().unwrap(),
            format!("{expected_y:.1}").parse::<f64>().unwrap(),
            "가시 run 기준선의 캐럿 위치",
        );
        assert_eq!(caret["height"], run["fontSize"]);
    }
}

#[test]
fn placement_transition_refreshes_last_section_without_note_residue() {
    let Fixture { mut doc, .. } = fixture();
    let original = protected_content(&doc);
    let last_shape = shape(&doc, 1);
    assert_placement(&doc, "documentEnd");
    for placement in ["sectionEnd", "documentEnd", "sectionEnd", "documentEnd"] {
        doc.apply_endnote_shape_native(0, &json!({"placement": placement}).to_string())
            .unwrap();
        assert_placement(&doc, placement);
        assert_eq!(protected_content(&doc), original);
        assert_eq!(shape(&doc, 1), last_shape, "다른 구역 모양은 바꾸지 않는다");
    }
    let before_shape = shape(&doc, 0);
    let before_layout = page_layouts(&doc);
    doc.apply_endnote_shape_native(0, "{}").unwrap();
    assert_eq!(shape(&doc, 0), before_shape, "빈 patch는 모양을 보존한다");
    assert_eq!(protected_content(&doc), original);
    assert_eq!(page_layouts(&doc), before_layout);
}

#[test]
fn placement_preserves_body_selection_and_supported_note_carets() {
    let Fixture { mut doc, controls } = fixture();
    let original = body_interaction(&doc);
    assert_note_carets(&doc, 1, controls[1]);
    doc.apply_endnote_shape_native(0, r#"{"placement":"sectionEnd"}"#)
        .unwrap();
    assert_placement(&doc, "sectionEnd");
    assert_eq!(body_interaction(&doc), original);
    assert_note_carets(&doc, 0, controls[0]);
    assert_note_carets(&doc, 1, controls[1]);
    doc.apply_endnote_shape_native(0, r#"{"placement":"documentEnd"}"#)
        .unwrap();
    assert_placement(&doc, "documentEnd");
    assert_eq!(body_interaction(&doc), original);
    assert_note_carets(&doc, 1, controls[1]);
}

#[test]
fn placement_snapshot_restore_recovers_both_sections_and_interactions() {
    let Fixture { mut doc, controls } = fixture();
    let original = protected_content(&doc);
    let original_interaction = body_interaction(&doc);
    let before = doc.save_snapshot_native();
    doc.apply_endnote_shape_native(0, r#"{"placement":"sectionEnd"}"#)
        .unwrap();
    assert_placement(&doc, "sectionEnd");
    let after = doc.save_snapshot_native();
    for _ in 0..2 {
        doc.restore_snapshot_native(before).unwrap();
        assert_placement(&doc, "documentEnd");
        assert_eq!(protected_content(&doc), original);
        assert_eq!(body_interaction(&doc), original_interaction);
        doc.restore_snapshot_native(after).unwrap();
        assert_placement(&doc, "sectionEnd");
        assert_eq!(protected_content(&doc), original);
        assert_eq!(body_interaction(&doc), original_interaction);
        assert_note_carets(&doc, 0, controls[0]);
        assert_note_carets(&doc, 1, controls[1]);
    }
}

#[test]
fn placement_roundtrips_both_formats_without_mutating_live_content() {
    for placement in ["documentEnd", "sectionEnd"] {
        let Fixture { mut doc, .. } = fixture();
        doc.apply_endnote_shape_native(0, &json!({"placement": placement}).to_string())
            .unwrap();
        assert_placement(&doc, placement);
        let live_model = format!("{:?}", doc.document());
        let live_interaction = body_interaction(&doc);
        for (name, bytes) in [
            ("HWP", doc.export_hwp_with_adapter_snapshot().unwrap()),
            ("HWPX", doc.export_hwpx_native().unwrap()),
        ] {
            let mut reopened = HwpDocument::from_bytes(&bytes).unwrap();
            assert_placement(&reopened, placement);
            let original = protected_content(&reopened);
            let interaction = body_interaction(&reopened);
            let other = if placement == "documentEnd" {
                "sectionEnd"
            } else {
                "documentEnd"
            };
            reopened
                .apply_endnote_shape_native(0, &json!({"placement": other}).to_string())
                .unwrap();
            assert_placement(&reopened, other);
            assert_eq!(
                protected_content(&reopened),
                original,
                "{name}: 재열기 뒤 전환"
            );
            assert_eq!(
                body_interaction(&reopened),
                interaction,
                "{name}: 본문 좌표"
            );
        }
        assert_eq!(
            format!("{:?}", doc.document()),
            live_model,
            "저장 사본만 사용"
        );
        assert_eq!(body_interaction(&doc), live_interaction);
        assert_placement(&doc, placement);
    }
}
