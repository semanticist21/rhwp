//! 개체 삭제가 남은 본문 앞/뒤 개체의 높이를 글줄에 더하지 않는 편집 계약.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::model::control::{Control, Equation};
use rhwp::model::footnote::Footnote;
use rhwp::model::image::Picture;
use rhwp::model::paragraph::{CharShapeRef, LineSeg, Paragraph};
use rhwp::model::shape::{
    CommonObjAttr, DrawingObjAttr, HorzRelTo, RectangleShape, ShapeObject, TextBox, TextWrap,
    VertRelTo,
};
use rhwp::model::style::{
    Alignment, BorderFill, BorderLine, BorderLineType, Fill, FillType, SolidFill,
};
use rhwp::model::table::{Cell, Table};
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use serde_json::Value;

#[derive(Clone, Copy, Debug)]
enum Kind {
    Picture,
    Shape,
    Equation,
}

const KINDS: [Kind; 3] = [Kind::Picture, Kind::Shape, Kind::Equation];
const WRAPS: [TextWrap; 2] = [TextWrap::InFrontOfText, TextWrap::BehindText];

fn object(kind: Kind, height: u32, tac: bool, wrap: TextWrap) -> Control {
    let common = CommonObjAttr {
        width: 4000,
        height,
        treat_as_char: tac,
        text_wrap: wrap,
        vert_rel_to: VertRelTo::Para,
        horz_rel_to: HorzRelTo::Para,
        horizontal_offset: 9000,
        vertical_offset: 6000,
        ..Default::default()
    };
    match kind {
        Kind::Picture => Control::Picture(Box::new(Picture {
            common,
            ..Default::default()
        })),
        Kind::Shape => Control::Shape(Box::new(ShapeObject::Rectangle(RectangleShape {
            common,
            ..Default::default()
        }))),
        Kind::Equation => Control::Equation(Box::new(Equation {
            common,
            script: "x".into(),
            font_size: 1000,
            ..Default::default()
        })),
    }
}

fn row(vpos: i32, height: i32) -> LineSeg {
    LineSeg {
        vertical_pos: vpos,
        line_height: height,
        text_height: height,
        baseline_distance: height * 85 / 100,
        line_spacing: height * 60 / 100,
        column_start: 123,
        segment_width: 42520,
        tag: LineSeg::TAG_SINGLE_SEGMENT_LINE,
        ..Default::default()
    }
}

fn paragraph(text: &str, shape: u32, vpos: i32, controls: Vec<Control>) -> Paragraph {
    let mut offset = 0;
    let char_offsets = text
        .chars()
        .map(|ch| {
            let current = offset;
            offset += ch.len_utf16() as u32;
            current
        })
        .collect();
    Paragraph {
        text: text.into(),
        char_offsets,
        char_count: offset + controls.len() as u32 * 8 + 1,
        char_shapes: vec![CharShapeRef {
            start_pos: 0,
            char_shape_id: shape,
        }],
        line_segs: vec![row(vpos, 1500)],
        controls,
        has_para_text: true,
        ..Default::default()
    }
}

fn fixture(deleted: Kind, remaining: Kind, tac: bool, wrap: TextWrap) -> DocumentCore {
    let mut core = DocumentCore::from_bytes(include_bytes!("../../saved/blank2010.hwp")).unwrap();
    let mut doc = core.document().clone();
    // 수리할 삭제·서식 API를 거치지 않고 15pt 글자모양과 저장 줄 원장을 설정한다.
    let mut shape = doc.doc_info.char_shapes[0].clone();
    shape.base_size = 1500;
    shape.raw_data = None;
    let shape_id = doc.doc_info.char_shapes.len() as u32;
    doc.doc_info.char_shapes.push(shape.clone());
    shape.base_size = 1100;
    shape.bold = true;
    let next_shape_id = doc.doc_info.char_shapes.len() as u32;
    doc.doc_info.char_shapes.push(shape);
    doc.doc_info.raw_stream = None;
    doc.sections[0].raw_stream = None;
    doc.sections[0].paragraphs.truncate(1);
    doc.sections[0].paragraphs[0].line_segs = vec![row(0, 1000)];
    doc.sections[0].paragraphs.push(paragraph(
        "A🦦B",
        shape_id,
        1600,
        vec![
            object(deleted, 1000, false, TextWrap::InFrontOfText),
            object(remaining, 3000, tac, wrap),
        ],
    ));
    let mut next = paragraph("다음 🦦 문단", next_shape_id, 4000, vec![]);
    next.line_segs = vec![row(4000, 1100)];
    doc.sections[0].paragraphs.push(next);
    core.set_document(doc);
    core
}

fn export(core: &DocumentCore, hwpx: bool) -> Vec<u8> {
    if hwpx {
        core.export_hwpx_native().unwrap()
    } else {
        core.export_hwp_native().unwrap()
    }
}

fn delete(core: &mut DocumentCore, kind: Kind) {
    match kind {
        Kind::Picture => core.delete_picture_control_native(0, 1, 0),
        Kind::Shape => core.delete_shape_control_native(0, 1, 0),
        Kind::Equation => core.delete_equation_control_native(0, 1, 0),
    }
    .unwrap();
}

#[derive(Debug, PartialEq)]
struct Scene {
    pages: u32,
    text: Vec<String>,
    glyphs: Vec<(String, [f64; 4])>,
    lines: Vec<[f64; 4]>,
    carets: Vec<Value>,
    props: Vec<Value>,
    objects: Vec<[f64; 4]>,
}

fn scene(core: &DocumentCore, remaining_index: usize) -> Scene {
    fn collect(node: &RenderNode, remaining: usize, scene: &mut Scene) {
        let rect = [node.bbox.x, node.bbox.y, node.bbox.width, node.bbox.height];
        let reference = match &node.node_type {
            RenderNodeType::Rectangle(shape) => shape.para_index.zip(shape.control_index),
            RenderNodeType::Image(image) => image.para_index.zip(image.control_index),
            RenderNodeType::Equation(eq) => eq.para_index.zip(eq.control_index),
            RenderNodeType::Placeholder(p) => p
                .control_ref
                .as_ref()
                .map(|r| (r.para_index, r.control_index)),
            _ => None,
        };
        if reference == Some((1, remaining)) {
            scene.objects.push(rect);
        }
        if let RenderNodeType::TextRun(run) = &node.node_type {
            if run.para_index.is_some() && !run.text.is_empty() {
                scene.glyphs.push((run.text.clone(), rect));
            }
        }
        if matches!(&node.node_type, RenderNodeType::TextLine(line) if line.para_index.is_some()) {
            scene.lines.push(rect);
        }
        for child in &node.children {
            collect(child, remaining, scene);
        }
    }
    let mut out = Scene {
        pages: core.page_count(),
        text: vec![],
        glyphs: vec![],
        lines: vec![],
        carets: vec![],
        props: vec![],
        objects: vec![],
    };
    for (pi, para) in core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .skip(1)
    {
        out.text.push(para.text.clone());
        for offset in 0..=para.text.chars().count() {
            out.carets.push(
                serde_json::from_str(&core.get_cursor_rect_native(0, pi, offset).unwrap()).unwrap(),
            );
            if offset < para.text.chars().count() {
                out.props.push(
                    serde_json::from_str(
                        &core.get_char_properties_at_native(0, pi, offset).unwrap(),
                    )
                    .unwrap(),
                );
            }
        }
    }
    for page in 0..out.pages {
        collect(
            &core.build_page_render_tree(page).unwrap().root,
            remaining_index,
            &mut out,
        );
    }
    out
}

fn assert_delete_preserves_scene(mut core: DocumentCore, deleted: Kind) {
    let before = scene(&core, 1);
    assert!(!before.glyphs.is_empty());
    assert!(
        !before.objects.is_empty(),
        "남은 개체의 실제 렌더 위치를 검사한다"
    );
    assert_eq!(before.text, ["A🦦B", "다음 🦦 문단"]);
    assert!(before.props[..3].iter().all(|p| p["fontSize"] == 1500));
    assert!(before.props[3..]
        .iter()
        .all(|p| p["fontSize"] == 1100 && p["bold"] == true));
    let para = &core.document().sections[0].paragraphs[1];
    let remaining = serde_json::to_value(&para.controls[1]).unwrap();
    let undo = core.save_snapshot_native();
    let baselines: Vec<_> = [false, true]
        .into_iter()
        .map(|hwpx| {
            let saved = DocumentCore::from_bytes(&export(&core, hwpx)).unwrap();
            scene(&saved, 1)
        })
        .collect();
    delete(&mut core, deleted);
    let para = &core.document().sections[0].paragraphs[1];
    assert_eq!(
        para.line_segs[0].line_height, 1500,
        "떠 있는 3000HU 개체가 15pt 글줄 높이를 바꾸면 안 된다"
    );
    assert_eq!(para.line_segs[0].text_height, 1500);
    assert_eq!(para.line_segs[0].vertical_pos, 1600);
    assert_eq!(para.line_segs[0].column_start, 123);
    assert_eq!(para.line_segs[0].segment_width, 42520);
    assert_eq!(para.char_offsets, [0, 1, 3]);
    assert_eq!(para.controls.len(), 1);
    assert_eq!(serde_json::to_value(&para.controls[0]).unwrap(), remaining);
    let after = scene(&core, 0);
    assert_eq!(
        after, before,
        "본문·캐럿·이웃 문단·남은 개체의 전체 위치와 모양을 보존한다"
    );
    for (hwpx, baseline) in [false, true].into_iter().zip(baselines) {
        let saved = DocumentCore::from_bytes(&export(&core, hwpx)).unwrap();
        assert_eq!(saved.document().sections[0].paragraphs[1].controls.len(), 1);
        assert_eq!(
            scene(&saved, 0),
            baseline,
            "형식별 무변경 저장본과 삭제 저장본의 배치를 대조한다"
        );
    }
    let redo = core.save_snapshot_native();
    core.restore_snapshot_native(undo).unwrap();
    assert_eq!(core.document().sections[0].paragraphs[1].controls.len(), 2);
    assert_eq!(scene(&core, 1), before);
    core.restore_snapshot_native(redo).unwrap();
    assert_eq!(core.document().sections[0].paragraphs[1].controls.len(), 1);
    assert_eq!(scene(&core, 0), after);
    core.discard_snapshot_native(undo);
    core.discard_snapshot_native(redo);
}

#[test]
fn raw_model_delete_preserves_floating_neighbors_and_text_flow() {
    for deleted in KINDS {
        for remaining in KINDS {
            for wrap in WRAPS {
                assert_delete_preserves_scene(fixture(deleted, remaining, false, wrap), deleted);
            }
        }
    }
}

#[test]
fn loaded_hwp_delete_preserves_floating_neighbors_and_text_flow() {
    for deleted in KINDS {
        for remaining in KINDS {
            for wrap in WRAPS {
                let core = fixture(deleted, remaining, false, wrap);
                assert_delete_preserves_scene(
                    DocumentCore::from_bytes(&export(&core, false)).unwrap(),
                    deleted,
                );
            }
        }
    }
}

#[test]
fn loaded_hwpx_delete_preserves_floating_neighbors_and_text_flow() {
    for deleted in KINDS {
        for remaining in KINDS {
            for wrap in WRAPS {
                let core = fixture(deleted, remaining, false, wrap);
                assert_delete_preserves_scene(
                    DocumentCore::from_bytes(&export(&core, true)).unwrap(),
                    deleted,
                );
            }
        }
    }
}

#[test]
fn tac_and_other_wrap_modes_keep_their_existing_height_contract() {
    for remaining in KINDS {
        for (tac, wraps) in [
            (true, WRAPS.as_slice()),
            (
                false,
                [
                    TextWrap::Square,
                    TextWrap::Tight,
                    TextWrap::Through,
                    TextWrap::TopAndBottom,
                ]
                .as_slice(),
            ),
        ] {
            for &wrap in wraps {
                let mut core = fixture(Kind::Shape, remaining, tac, wrap);
                delete(&mut core, Kind::Shape);
                assert_eq!(
                    core.document().sections[0].paragraphs[1].line_segs[0].line_height,
                    3000,
                    "{remaining:?} {tac} {wrap:?}"
                );
            }
        }
    }
}

#[test]
fn taller_overlay_does_not_override_remaining_tac_object_height() {
    for remaining in KINDS {
        for wrap in WRAPS {
            let mut core = fixture(Kind::Shape, remaining, true, wrap);
            let mut doc = core.document().clone();
            let para = &mut doc.sections[0].paragraphs[1];
            para.controls.push(object(Kind::Shape, 9000, false, wrap));
            para.char_count += 8;
            core.set_document(doc);
            delete(&mut core, Kind::Shape);
            assert_eq!(
                core.document().sections[0].paragraphs[1].line_segs[0].line_height,
                3000
            );
        }
    }
}

#[test]
fn empty_host_keeps_default_line_height_with_floating_neighbor() {
    for remaining in KINDS {
        for wrap in WRAPS {
            let mut core = fixture(Kind::Shape, remaining, false, wrap);
            let mut doc = core.document().clone();
            let para = &mut doc.sections[0].paragraphs[1];
            para.text.clear();
            para.char_offsets.clear();
            para.char_count = 17;
            core.set_document(doc);
            delete(&mut core, Kind::Shape);
            let ls = &core.document().sections[0].paragraphs[1].line_segs[0];
            assert_eq!(
                (
                    ls.line_height,
                    ls.text_height,
                    ls.baseline_distance,
                    ls.line_spacing
                ),
                (1000, 1000, 850, 600)
            );
            assert_eq!(ls.vertical_pos, 1600);
        }
    }
}

#[test]
fn empty_overlay_host_keeps_its_saved_body_cursor_row() {
    let mut failures = Vec::new();
    for kind in KINDS {
        for wrap in WRAPS {
            for rel in [VertRelTo::Para, VertRelTo::Paper] {
                for right_margin in [0, -1200] {
                    let mut core =
                        DocumentCore::from_bytes(include_bytes!("../../saved/blank2010.hwp"))
                            .unwrap();
                    let mut doc = core.document().clone();
                    doc.sections[0].raw_stream = None;
                    doc.sections[0].paragraphs.truncate(1);
                    let host = &mut doc.sections[0].paragraphs[0];
                    assert!(host.text.is_empty());
                    host.line_segs = vec![row(0, 1000)];
                    let para_shape = &mut doc.doc_info.para_shapes[host.para_shape_id as usize];
                    para_shape.alignment = Alignment::Left;
                    para_shape.margin_right = right_margin;
                    core.set_document(doc.clone());
                    // 개체를 추가하기 전 정상 빈 문단의 저장 줄이 독립적인 기준이다.
                    let expected: Value =
                        serde_json::from_str(&core.get_cursor_rect_native(0, 0, 0).unwrap())
                            .unwrap();
                    let baselines: Vec<_> = [false, true]
                        .into_iter()
                        .map(|hwpx| {
                            let saved = DocumentCore::from_bytes(&export(&core, hwpx)).unwrap();
                            serde_json::from_str::<Value>(
                                &saved.get_cursor_rect_native(0, 0, 0).unwrap(),
                            )
                            .unwrap()
                        })
                        .collect();
                    let mut overlay = object(kind, 3000, false, wrap);
                    let common = match &mut overlay {
                        Control::Picture(pic) => &mut pic.common,
                        Control::Shape(shape) => shape.common_mut(),
                        Control::Equation(eq) => &mut eq.common,
                        _ => unreachable!(),
                    };
                    common.vert_rel_to = rel;
                    common.horz_rel_to = HorzRelTo::Paper;
                    common.horizontal_offset = 15000;
                    common.vertical_offset = 15000;
                    let host = &mut doc.sections[0].paragraphs[0];
                    host.controls.push(overlay);
                    host.char_count += 8;
                    core.set_document(doc);
                    let cursor = core.get_cursor_rect_native(0, 0, 0);
                    match cursor {
                        Ok(cursor) => {
                            let cursor: Value = serde_json::from_str(&cursor).unwrap();
                            assert_eq!(cursor, expected, "{kind:?} {wrap:?} {rel:?}");
                            let x = expected["x"].as_f64().unwrap();
                            let y = expected["y"].as_f64().unwrap()
                                + expected["height"].as_f64().unwrap() / 2.0;
                            let hit: Value =
                                serde_json::from_str(&core.hit_test_native(0, x, y).unwrap())
                                    .unwrap();
                            assert_eq!(hit["sectionIndex"], 0);
                            assert_eq!(hit["paragraphIndex"], 0);
                            assert_eq!(hit["charOffset"], 0);
                        }
                        Err(error) => failures.push(format!("{kind:?} {wrap:?} {rel:?}: {error}")),
                    }
                    for (hwpx, baseline) in [false, true].into_iter().zip(baselines) {
                        let saved = DocumentCore::from_bytes(&export(&core, hwpx)).unwrap();
                        let cursor = saved.get_cursor_rect_native(0, 0, 0);
                        match cursor {
                            Ok(cursor) => assert_eq!(
                                serde_json::from_str::<Value>(&cursor).unwrap(),
                                baseline,
                                "{kind:?} {wrap:?} {rel:?} hwpx={hwpx}"
                            ),
                            Err(error) => failures
                                .push(format!("{kind:?} {wrap:?} {rel:?} hwpx={hwpx}: {error}")),
                        }
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn empty_tac_host_keeps_distinct_inline_cursor_slots() {
    for kind in KINDS {
        for wrap in WRAPS {
            let mut core =
                DocumentCore::from_bytes(include_bytes!("../../saved/blank2010.hwp")).unwrap();
            let mut doc = core.document().clone();
            doc.sections[0].raw_stream = None;
            doc.sections[0].paragraphs.truncate(1);
            let host = &mut doc.sections[0].paragraphs[0];
            assert!(host.text.is_empty());
            host.line_segs = vec![row(0, 3000)];
            host.controls.push(object(kind, 3000, true, wrap));
            host.char_count += 8;
            core.set_document(doc);
            for saved in [None, Some(false), Some(true)] {
                let loaded;
                let core = if let Some(hwpx) = saved {
                    loaded = DocumentCore::from_bytes(&export(&core, hwpx)).unwrap();
                    &loaded
                } else {
                    &core
                };
                let before: Value =
                    serde_json::from_str(&core.get_cursor_rect_native(0, 0, 0).unwrap()).unwrap();
                let after: Value =
                    serde_json::from_str(&core.get_cursor_rect_native(0, 0, 1).unwrap()).unwrap();
                assert_eq!(before["pageIndex"], after["pageIndex"]);
                assert!(
                    after["x"].as_f64().unwrap() > before["x"].as_f64().unwrap(),
                    "{kind:?} {wrap:?} {saved:?}: {before} / {after}"
                );
            }
        }
    }
}

#[test]
fn off_page_saved_overlay_host_does_not_gain_a_body_cursor_row() {
    for kind in [Kind::Picture, Kind::Shape] {
        for wrap in WRAPS {
            let mut core =
                DocumentCore::from_bytes(include_bytes!("../../saved/blank2010.hwp")).unwrap();
            let mut doc = core.document().clone();
            doc.sections[0].raw_stream = None;
            doc.sections[0].paragraphs.truncate(1);
            let host = &mut doc.sections[0].paragraphs[0];
            assert!(host.text.is_empty());
            // 저장 빈 줄은 쪽 높이보다 아래에 있고 개체 자체는 첫 쪽 안에 있다.
            host.line_segs = vec![row(100000, 1000)];
            let mut overlay = object(kind, 3000, false, wrap);
            let common = match &mut overlay {
                Control::Picture(pic) => &mut pic.common,
                Control::Shape(shape) => shape.common_mut(),
                _ => unreachable!(),
            };
            common.vert_rel_to = VertRelTo::Paper;
            common.horz_rel_to = HorzRelTo::Paper;
            common.horizontal_offset = 15000;
            common.vertical_offset = 15000;
            host.controls.push(overlay);
            host.char_count += 8;
            core.set_document(doc);
            assert!(
                core.get_cursor_rect_native(0, 0, 0).is_err(),
                "{kind:?} {wrap:?}: 쪽 밖의 저장 줄을 본문 캐럿으로 만들지 않는다"
            );
        }
    }
}

#[test]
fn empty_overlay_cursor_rows_keep_shape_only_paint_and_flow_ownership() {
    use rhwp::renderer::{
        composer::compose_paragraph,
        height_measurer::HeightMeasurer,
        layout::LayoutEngine,
        pagination::{PageItem, Paginator},
        style_resolver::resolve_styles,
    };
    fn paint(node: &RenderNode, out: &mut Vec<Value>) {
        if matches!(&node.node_type, RenderNodeType::TextLine(line) if line.para_index == Some(0)) {
            return;
        }
        if matches!(
            &node.node_type,
            RenderNodeType::TextRun(_)
                | RenderNodeType::TextLine(_)
                | RenderNodeType::Rectangle(_)
                | RenderNodeType::Line(_)
                | RenderNodeType::Image(_)
                | RenderNodeType::Equation(_)
                | RenderNodeType::Placeholder(_)
        ) {
            out.push(serde_json::json!({ "type": node.node_type, "bbox": node.bbox }));
        }
        for child in &node.children {
            paint(child, out);
        }
    }
    for kind in [Kind::Picture, Kind::Shape] {
        for wrap in WRAPS {
            for vpos in [1600, 100000] {
                let source =
                    DocumentCore::from_bytes(include_bytes!("../../saved/blank2010.hwp")).unwrap();
                let mut doc = source.document().clone();
                doc.sections[0].paragraphs.truncate(1);
                doc.doc_info.border_fills.push(BorderFill {
                    borders: [BorderLine {
                        line_type: BorderLineType::Solid,
                        width: 3,
                        color: 0x123456,
                    }; 4],
                    fill: Fill {
                        fill_type: FillType::Solid,
                        solid: Some(SolidFill {
                            background_color: 0xFEDCBA,
                            pattern_color: 0,
                            pattern_type: 0,
                        }),
                        ..Default::default()
                    },
                    ..Default::default()
                });
                let mut style = doc.doc_info.para_shapes[0].clone();
                style.border_fill_id = doc.doc_info.border_fills.len() as u16;
                style.margin_right = -1200;
                style.alignment = Alignment::Left;
                let style_id = doc.doc_info.para_shapes.len() as u16;
                doc.doc_info.para_shapes.push(style);
                let host = &mut doc.sections[0].paragraphs[0];
                host.para_shape_id = style_id;
                host.line_segs = vec![row(vpos, 1000)];
                let mut overlay = object(kind, 3000, false, wrap);
                let common = match &mut overlay {
                    Control::Picture(pic) => &mut pic.common,
                    Control::Shape(shape) => shape.common_mut(),
                    _ => unreachable!(),
                };
                common.vert_rel_to = VertRelTo::Paper;
                common.horz_rel_to = HorzRelTo::Paper;
                common.horizontal_offset = 15000;
                common.vertical_offset = 15000;
                let ci = host.controls.len();
                host.controls.push(overlay);
                host.char_count += 8;
                doc.sections[0]
                    .paragraphs
                    .push(paragraph("이웃 A🦦B", 0, vpos, vec![]));
                let paragraphs = &doc.sections[0].paragraphs;
                let styles = resolve_styles(&doc.doc_info, 96.0);
                let composed = paragraphs.iter().map(compose_paragraph).collect::<Vec<_>>();
                let measured =
                    HeightMeasurer::new(96.0).measure_section(paragraphs, &composed, &styles, None);
                let mut pages = Paginator::new(96.0).paginate_with_measured(
                    paragraphs,
                    &measured,
                    &Default::default(),
                    &Default::default(),
                    0,
                    &styles.para_styles,
                );
                let page = &mut pages.pages[0];
                let engine = LayoutEngine::new(96.0);
                let mut outputs = Vec::new();
                for include_cursor_host in [false, true] {
                    page.column_contents[0].items = vec![
                        PageItem::Shape {
                            para_index: 0,
                            control_index: ci,
                        },
                        PageItem::FullParagraph { para_index: 1 },
                    ];
                    if include_cursor_host {
                        page.column_contents[0]
                            .items
                            .insert(0, PageItem::FullParagraph { para_index: 0 });
                    }
                    // 저장 사다리 델타가 0인 부동 host의 기존 Shape-only paint/flow가 기준이다.
                    let tree = engine.build_render_tree(
                        page,
                        paragraphs,
                        &[],
                        &[],
                        &composed,
                        &styles,
                        &Default::default(),
                        &[],
                        None,
                        &measured.tables,
                        None,
                        0,
                        &[],
                    );
                    let mut nodes = Vec::new();
                    paint(&tree.root, &mut nodes);
                    let overflows = format!("{:?}", engine.take_overflows());
                    outputs.push((nodes, overflows));
                }
                assert_eq!(outputs[1], outputs[0], "{kind:?} {wrap:?} vpos={vpos}");
            }
        }
    }
}

#[test]
fn zero_saved_width_does_not_publish_reflowed_rows() {
    let mut core = fixture(Kind::Shape, Kind::Picture, false, TextWrap::InFrontOfText);
    core.document_mut().sections[0].paragraphs[1].line_segs[0].segment_width = 0;
    let before = core.document().sections[0].paragraphs[1].line_segs.clone();
    delete(&mut core, Kind::Shape);
    assert_eq!(core.document().sections[0].paragraphs[1].line_segs, before);
    assert_eq!(core.document().sections[0].paragraphs[1].text, "A🦦B");
    assert_eq!(core.document().sections[0].paragraphs[1].controls.len(), 1);
}

#[test]
fn multiline_loaded_body_preserves_every_row_and_neighbor_after_delete() {
    for hwpx in [false, true] {
        let mut core = fixture(Kind::Shape, Kind::Shape, false, TextWrap::InFrontOfText);
        let mut doc = core.document().clone();
        let host = &mut doc.sections[0].paragraphs[1];
        *host = paragraph(
            "A🦦B\nC",
            host.char_shapes[0].char_shape_id,
            1600,
            host.controls.clone(),
        );
        let mut second = row(4000, 1500);
        second.text_start = 5;
        host.line_segs.push(second);
        doc.sections[0].paragraphs[2].line_segs[0].vertical_pos = 6400;
        core.set_document(doc);
        let mut loaded = DocumentCore::from_bytes(&export(&core, hwpx)).unwrap();
        let before = scene(&loaded, 1);
        assert_eq!(before.text[0], "A🦦B\nC");
        assert!(before.glyphs.iter().any(|(text, _)| text == "C"));
        let saved_before = DocumentCore::from_bytes(&export(&loaded, hwpx)).unwrap();
        let baseline = scene(&saved_before, 1);
        delete(&mut loaded, Kind::Shape);
        assert_eq!(scene(&loaded, 0), before);
        let rows = &loaded.document().sections[0].paragraphs[1].line_segs;
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|line| line.line_height == 1500));
        assert_eq!(rows[0].vertical_pos, 1600);
        assert_eq!(rows[1].vertical_pos, 4000);
        let reopened = DocumentCore::from_bytes(&export(&loaded, hwpx)).unwrap();
        assert_eq!(scene(&reopened, 0), baseline);
    }
}

#[test]
fn footnote_deletion_keeps_floating_neighbor_out_of_host_height() {
    for wrap in WRAPS {
        let mut core = fixture(Kind::Shape, Kind::Shape, false, wrap);
        let mut doc = core.document().clone();
        let para = &mut doc.sections[0].paragraphs[1];
        para.controls[0] = Control::Footnote(Box::new(Footnote {
            number: 1,
            paragraphs: vec![paragraph("각주", 0, 0, vec![])],
            ..Default::default()
        }));
        core.set_document(doc);
        core.delete_footnote_native(0, 1, 0).unwrap();
        let para = &core.document().sections[0].paragraphs[1];
        assert_eq!(para.line_segs[0].line_height, 1500);
        assert_eq!(para.line_segs[0].vertical_pos, 1600);
        assert_eq!(para.text, "A🦦B");
        assert_eq!(para.controls.len(), 1);
    }
}

fn leaf(core: &DocumentCore, textbox: bool) -> &Paragraph {
    match &core.document().sections[0].paragraphs[1].controls[0] {
        Control::Table(table) if !textbox => &table.cells[0].paragraphs[0],
        Control::Shape(shape) => match shape.as_ref() {
            ShapeObject::Rectangle(rect) => &rect.drawing.text_box.as_ref().unwrap().paragraphs[0],
            _ => panic!("글상자 사각형"),
        },
        _ => panic!("셀 또는 글상자"),
    }
}

#[test]
fn by_path_deletion_reflows_leaf_cell_and_textbox_with_floating_neighbor() {
    for textbox in [false, true] {
        for wrap in WRAPS {
            let mut core = fixture(Kind::Picture, Kind::Shape, false, wrap);
            let mut doc = core.document().clone();
            let target = doc.sections[0].paragraphs[1].clone();
            let common = CommonObjAttr {
                width: 42520,
                height: 14000,
                treat_as_char: true,
                ..Default::default()
            };
            let container = if textbox {
                Control::Shape(Box::new(ShapeObject::Rectangle(RectangleShape {
                    common,
                    drawing: DrawingObjAttr {
                        text_box: Some(TextBox {
                            max_width: 42520,
                            paragraphs: vec![target],
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    ..Default::default()
                })))
            } else {
                Control::Table(Box::new(Table {
                    common,
                    row_count: 1,
                    col_count: 1,
                    cells: vec![Cell {
                        row_span: 1,
                        col_span: 1,
                        width: 42520,
                        height: 14000,
                        paragraphs: vec![target],
                        ..Default::default()
                    }],
                    ..Default::default()
                }))
            };
            doc.sections[0].paragraphs[1] = paragraph("", 0, 1600, vec![container]);
            core.set_document(doc);
            let text = leaf(&core, textbox).text.clone();
            let remaining = serde_json::to_value(&leaf(&core, textbox).controls[1]).unwrap();
            core.delete_cell_picture_control_by_path_native(
                0,
                1,
                r#"[{"controlIndex":0,"cellIndex":0,"cellParaIndex":0}]"#,
                0,
            )
            .unwrap();
            let para = leaf(&core, textbox);
            assert_eq!(para.text, text);
            assert_eq!(para.char_offsets, [0, 1, 3]);
            assert_eq!(para.line_segs[0].line_height, 1500);
            assert_eq!(para.line_segs[0].vertical_pos, 1600);
            assert_eq!(serde_json::to_value(&para.controls[0]).unwrap(), remaining);
        }
    }
}
