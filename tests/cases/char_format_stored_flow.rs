//! 본문 서식 재조판의 저장 원점·이웃 흐름·직접 모양 복원 계약.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::model::paragraph::LineSeg;
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use serde_json::Value;

#[derive(Debug, PartialEq)]
struct Scene {
    pages: u32,
    svg: Vec<String>,
    glyphs: Vec<(usize, String, [f64; 4])>,
    carets: Vec<(usize, Value)>,
    text: Vec<String>,
    props: Vec<Value>,
}

fn scene(core: &DocumentCore) -> Scene {
    fn collect(node: &RenderNode, out: &mut Vec<(usize, String, [f64; 4])>) {
        if let RenderNodeType::TextRun(run) = &node.node_type {
            if let Some(para) = run.para_index.filter(|_| !run.text.is_empty()) {
                out.push((
                    para,
                    run.text.clone(),
                    [node.bbox.x, node.bbox.y, node.bbox.width, node.bbox.height],
                ));
            }
        }
        for child in &node.children {
            collect(child, out);
        }
    }
    let mut glyphs = Vec::new();
    let svg = (0..core.page_count())
        .map(|page| {
            collect(
                &core.build_page_render_tree(page).unwrap().root,
                &mut glyphs,
            );
            core.render_page_svg_native(page).unwrap()
        })
        .collect();
    let paragraphs = &core.document().sections[0].paragraphs;
    Scene {
        pages: core.page_count(),
        svg,
        glyphs,
        carets: paragraphs
            .iter()
            .enumerate()
            .filter(|(_, para)| !para.text.is_empty())
            .map(|(index, _)| {
                (
                    index,
                    serde_json::from_str(&core.get_cursor_rect_native(0, index, 1).unwrap())
                        .unwrap(),
                )
            })
            .collect(),
        text: paragraphs.iter().map(|para| para.text.clone()).collect(),
        props: (0..paragraphs.len())
            .map(|index| {
                serde_json::from_str(&core.get_char_properties_at_native(0, index, 0).unwrap())
                    .unwrap()
            })
            .collect(),
    }
}

fn row(vpos: i32, height: i32) -> LineSeg {
    LineSeg {
        vertical_pos: vpos,
        line_height: height,
        text_height: height,
        baseline_distance: height * 85 / 100,
        line_spacing: height * 60 / 100,
        segment_width: 42520,
        tag: LineSeg::TAG_SINGLE_SEGMENT_LINE,
        ..Default::default()
    }
}

fn export(core: &DocumentCore, hwpx: bool) -> Vec<u8> {
    if hwpx {
        core.export_hwpx_native().unwrap()
    } else {
        core.export_hwp_native().unwrap()
    }
}

fn fixture(missing_first_row: bool, hwpx: bool) -> DocumentCore {
    let mut core = DocumentCore::from_bytes(include_bytes!("../../saved/blank2010.hwp")).unwrap();
    let target = usize::from(!missing_first_row);
    for index in 1..=target + 1 {
        core.insert_paragraph_native(0, index).unwrap();
    }
    core.insert_text_native(0, target, 0, "AB").unwrap();
    core.insert_text_native(0, target + 1, 0, "N").unwrap();
    // 생성에 수리할 서식 API를 쓰지 않는다. 첫 문단 줄 기록이 없는 실제 HWPX와
    // 원점이 있는 HWP 입력을 원문 모델로 독립 설정한다.
    let base = core.document().sections[0].paragraphs[target]
        .char_shape_id_at(0)
        .unwrap();
    let mut shape = core.document().doc_info.char_shapes[base as usize].clone();
    shape.base_size = 1200;
    let shape_id = core.document().doc_info.char_shapes.len() as u32;
    core.document_mut().doc_info.char_shapes.push(shape);
    core.document_mut().doc_info.raw_stream = None;
    let section = &mut core.document_mut().sections[0];
    section.raw_stream = None;
    section.paragraphs[target].apply_char_shape_range(0, 2, shape_id);
    if missing_first_row {
        section.paragraphs[0].line_segs.clear();
        section.paragraphs[1].line_segs = vec![row(1600, 1000)];
    } else {
        section.paragraphs[0].line_segs = vec![row(0, 1000)];
        section.paragraphs[1].line_segs = vec![row(1600, 1200)];
        section.paragraphs[2].line_segs = vec![row(3520, 1000)];
    }
    DocumentCore::from_bytes(&export(&core, hwpx)).unwrap()
}

#[test]
fn hwpx_noop_offset_keeps_missing_row_neighbor_and_saved_scene() {
    let mut core = fixture(true, true);
    assert!(core.document().sections[0].paragraphs[0]
        .line_segs
        .is_empty());
    assert_eq!(
        core.document().sections[0].paragraphs[1].line_segs[0].vertical_pos,
        1600
    );
    let before = scene(&core);
    assert_eq!(before.text, ["AB", "N"]);
    assert_eq!(before.props[0]["fontSize"], 1200);
    assert_eq!(
        before.props[0]["charOffsets"],
        serde_json::json!([0, 0, 0, 0, 0, 0, 0])
    );
    core.apply_char_format_native(0, 0, 0, 2, r#"{"charOffsets":[0,0,0,0,0,0,0]}"#)
        .unwrap();
    assert_eq!(
        scene(&core),
        before,
        "같은 값으로 글자·캐럿·이웃 문단을 이동하지 않는다"
    );
    let reopened = DocumentCore::from_bytes(&export(&core, true)).unwrap();
    assert_eq!(
        scene(&reopened),
        before,
        "저장해 다시 열어도 같은 배치를 유지한다"
    );
}

#[test]
fn stored_origin_size_change_and_shape_id_undo_redo_preserve_flow() {
    for hwpx in [false, true] {
        let mut core = fixture(false, hwpx);
        let before = scene(&core);
        let undo = core.save_snapshot_native();
        let old_id = core.document().sections[0].paragraphs[1]
            .char_shape_id_at(0)
            .unwrap();
        core.apply_char_format_native(0, 1, 0, 2, r#"{"charOffsets":[0,0,0,0,0,0,0]}"#)
            .unwrap();
        assert_eq!(
            scene(&core),
            before,
            "저장 원점에서 같은 값은 배치를 보존한다"
        );
        core.apply_char_format_native(0, 1, 0, 2, r#"{"fontSize":1800}"#)
            .unwrap();
        let changed = scene(&core);
        let redo = core.save_snapshot_native();
        let new_id = core.document().sections[0].paragraphs[1]
            .char_shape_id_at(0)
            .unwrap();
        assert_ne!(old_id, new_id);
        assert_eq!(changed.pages, 1);
        assert_eq!(changed.text, before.text);
        assert_eq!(changed.props[1]["fontSize"], 1800);
        assert_eq!(changed.props[2], before.props[2]);
        assert_eq!(
            core.document().sections[0].paragraphs[1].line_segs[0].vertical_pos,
            1600
        );
        assert_eq!(
            core.document().sections[0].paragraphs[1].line_segs[0].line_height,
            1800
        );
        assert_eq!(
            core.document().sections[0].paragraphs[2].line_segs[0].vertical_pos,
            4480
        );
        let old_neighbor = before.glyphs.iter().find(|run| run.0 == 2).unwrap();
        let new_neighbor = changed.glyphs.iter().find(|run| run.0 == 2).unwrap();
        assert!((new_neighbor.2[1] - old_neighbor.2[1] - 12.8).abs() < 1e-8);
        let saved = DocumentCore::from_bytes(&export(&core, hwpx)).unwrap();
        assert_eq!(
            scene(&saved),
            changed,
            "실제 글자 높이와 이웃 전진량을 함께 저장한다"
        );
        core.set_char_shape_id_native(0, 1, 0, 2, old_id).unwrap();
        assert_eq!(
            scene(&core),
            before,
            "서식 ID 복원도 원점과 이웃 흐름을 복원한다"
        );
        core.set_char_shape_id_native(0, 1, 0, 2, new_id).unwrap();
        assert_eq!(
            scene(&core),
            changed,
            "다시 적용하면 같은 글자 높이와 흐름이 된다"
        );
        core.restore_snapshot_native(undo).unwrap();
        assert_eq!(
            scene(&core),
            before,
            "스냅샷 실행 취소는 전체 장면을 복원한다"
        );
        core.restore_snapshot_native(redo).unwrap();
        assert_eq!(
            scene(&core),
            changed,
            "스냅샷 다시 실행은 전체 장면을 복원한다"
        );
        core.discard_snapshot_native(undo);
        core.discard_snapshot_native(redo);
    }
}

#[test]
fn character_size_after_empty_paragraph_keeps_one_page() {
    // upstream 606의 빈 문단 뒤 크기 변경 반례를 같은 본문 API로 검증한다.
    let mut core = DocumentCore::from_bytes(include_bytes!("../../saved/blank2010.hwp")).unwrap();
    core.insert_text_native(0, 0, 0, "Title").unwrap();
    core.apply_char_format_native(0, 0, 0, 5, r#"{"fontSize":2000}"#)
        .unwrap();
    for (index, text) in ["A", "B", "", "C", "D"].iter().enumerate() {
        core.insert_paragraph_native(0, index + 1).unwrap();
        if !text.is_empty() {
            core.insert_text_native(0, index + 1, 0, text).unwrap();
        }
    }
    for index in [2, 4] {
        core.apply_char_format_native(0, index, 0, 1, r#"{"fontSize":1300,"bold":true}"#)
            .unwrap();
    }
    let paragraphs = &core.document().sections[0].paragraphs;
    let empty = paragraphs[3].line_segs.last().unwrap();
    assert!(paragraphs[4].line_segs[0].vertical_pos >= empty.vertical_pos + empty.line_height);
    assert_eq!(core.page_count(), 1);
    for hwpx in [false, true] {
        let saved = DocumentCore::from_bytes(&export(&core, hwpx)).unwrap();
        assert_eq!(
            saved.page_count(),
            1,
            "빈 문단 뒤 서식은 저장 후에도 쪽을 만들지 않는다"
        );
        assert_eq!(
            saved.document().sections[0]
                .paragraphs
                .iter()
                .map(|p| p.text.as_str())
                .collect::<Vec<_>>(),
            ["Title", "A", "B", "", "C", "D"]
        );
    }
}
