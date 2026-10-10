//! 스타일의 왼쪽 탭 정의를 본문·일반 셀에 연결하는 공개 편집·그리기·저장 계약.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::control::Control;
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;
use serde_json::{json, Value};

const STOP: u32 = 15000;
const TAB_TEXT: &str = "A\tB";

fn value(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

struct Fixture {
    doc: HwpDocument,
    style: u32,
    parent: usize,
    control: usize,
}

fn fixture() -> Fixture {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, TAB_TEXT).unwrap();
    doc.insert_paragraph_native(0, 1).unwrap();
    doc.insert_text_native(0, 1, 0, TAB_TEXT).unwrap();
    doc.insert_paragraph_native(0, 2).unwrap();
    let created = value(&doc.create_table_native(0, 2, 0, 1, 2).unwrap());
    let parent = created["paraIdx"].as_u64().unwrap() as usize;
    let control = created["controlIdx"].as_u64().unwrap() as usize;
    for cell in 0..2 {
        doc.set_cell_properties_native(
            0,
            parent,
            control,
            cell,
            r#"{"width":15000,"paddingLeft":0,"paddingRight":0,"paddingTop":0,"paddingBottom":0,"applyInnerMargin":true}"#,
        )
        .unwrap();
        doc.insert_text_in_cell_native(0, parent, control, cell, 0, 0, TAB_TEXT)
            .unwrap();
    }
    let base = doc.document().doc_info.styles[0].clone();
    let style = doc.create_style(
        &json!({"name":"왼쪽 탭","type":0,"baseParaShapeId":base.para_shape_id,
            "baseCharShapeId":base.char_shape_id})
        .to_string(),
    );
    assert!(style > 0);
    // 빈 문서의 같은 문단 모양을 쓰는 일반 셀을 준비한다. 직접 서식은 별도 시험에서 준다.
    let mut model = doc.document().clone();
    let Control::Table(table) = &mut model.sections[0].paragraphs[parent].controls[control] else {
        panic!("표 없음");
    };
    for cell in &mut table.cells {
        cell.paragraphs[0].para_shape_id = base.para_shape_id;
    }
    doc.set_document(model);
    for para in 0..2 {
        doc.apply_style_native(0, para, style as usize).unwrap();
    }
    for cell in 0..2 {
        doc.apply_cell_style_native(0, parent, control, cell, 0, style as usize)
            .unwrap();
    }
    Fixture {
        doc,
        style: style as u32,
        parent,
        control,
    }
}

fn body_props(doc: &HwpDocument, para: usize) -> Value {
    value(&doc.get_para_properties_at_native(0, para).unwrap())
}

fn cell_props(doc: &HwpDocument, f: &Fixture, cell: usize) -> Value {
    value(
        &doc.get_cell_para_properties_at_native(0, f.parent, f.control, cell, 0)
            .unwrap(),
    )
}

fn caret_x(doc: &HwpDocument, f: &Fixture, cell: Option<usize>, offset: usize) -> f64 {
    let rect = if let Some(cell) = cell {
        doc.get_cursor_rect_in_cell_native(0, f.parent, f.control, cell, 0, offset)
    } else {
        doc.get_cursor_rect_native(0, 0, offset)
    };
    value(&rect.unwrap())["x"].as_f64().unwrap()
}

fn leader_fills(doc: &HwpDocument, f: &Fixture, cell: Option<usize>) -> Vec<u8> {
    fn collect(node: &RenderNode, f: &Fixture, cell: Option<usize>, out: &mut Vec<u8>) {
        if let RenderNodeType::TextRun(run) = &node.node_type {
            let owned = match (&run.cell_context, cell) {
                (None, None) => run.section_index == Some(0) && run.para_index == Some(0),
                (Some(ctx), Some(cell)) => {
                    run.section_index == Some(0)
                        && ctx.parent_para_index == f.parent
                        && ctx.path.len() == 1
                        && ctx.path[0].control_index == f.control
                        && ctx.path[0].cell_index == cell
                        && ctx.path[0].cell_para_index == 0
                }
                _ => false,
            };
            if owned {
                for leader in &run.style.tab_leaders {
                    assert!(
                        leader.end_x > leader.start_x,
                        "채움선은 실제 탭 간격을 채운다"
                    );
                    out.push(leader.fill_type);
                }
            }
        }
        for child in &node.children {
            collect(child, f, cell, out);
        }
    }
    let mut fills = Vec::new();
    collect(
        &doc.build_page_render_tree(0).unwrap().root,
        f,
        cell,
        &mut fills,
    );
    fills
}

fn assert_tabs(props: &Value, fill: u8) {
    assert_eq!(
        props["tabStops"],
        json!([{"position":STOP,"type":0,"fill":fill}])
    );
}

fn assert_layout(doc: &HwpDocument, f: &Fixture, fill: u8) {
    assert_tabs(&value(&doc.get_style_detail(f.style))["paraProps"], fill);
    assert_tabs(&body_props(doc, 0), fill);
    assert_tabs(&cell_props(doc, f, 0), fill);
    for cell in [None, Some(0)] {
        // 탭 위치는 문단 여백처럼 2배 HWPUNIT다. 15000은 실제 100px 자리다.
        let origin = caret_x(doc, f, cell, 0);
        assert!((caret_x(doc, f, cell, 2) - origin - 100.0).abs() < 0.2);
        let fills = leader_fills(doc, f, cell);
        assert!(!fills.is_empty(), "실제 렌더 트리에 채움선이 있어야 한다");
        assert!(fills.into_iter().all(|actual| actual == fill));
    }
}

fn reopen(doc: &HwpDocument, hwpx: bool) -> HwpDocument {
    HwpDocument::from_bytes(&if hwpx {
        doc.export_hwpx_native().unwrap()
    } else {
        doc.export_hwp_native().unwrap()
    })
    .unwrap()
}

#[test]
fn left_tab_and_leader_propagate_render_and_roundtrip_with_snapshot_history() {
    let mut f = fixture();
    let before = f.doc.save_snapshot_native();
    let old_x = caret_x(&f.doc, &f, None, 2);
    let original_style = f.doc.document().doc_info.styles[f.style as usize].clone();
    let mut original_shape = serde_json::to_value(
        &f.doc.document().doc_info.para_shapes[original_style.para_shape_id as usize],
    )
    .unwrap();
    assert!(f.doc.update_style_shapes(
        f.style,
        "{}",
        &json!({"tabStops":[{"position":STOP,"type":0,"fill":3}]}).to_string()
    ));
    assert_layout(&f.doc, &f, 3);
    assert_ne!(caret_x(&f.doc, &f, None, 2), old_x);
    assert_eq!(
        f.doc.document().doc_info.styles[f.style as usize].char_shape_id,
        original_style.char_shape_id
    );
    let new_style = &f.doc.document().doc_info.styles[f.style as usize];
    let mut new_shape = serde_json::to_value(
        &f.doc.document().doc_info.para_shapes[new_style.para_shape_id as usize],
    )
    .unwrap();
    for shape in [&mut original_shape, &mut new_shape] {
        shape.as_object_mut().unwrap().remove("tab_def_id");
        shape.as_object_mut().unwrap().remove("raw_data");
    }
    assert_eq!(new_shape, original_shape, "탭 외 문단 모양은 보존한다");
    let applied = f.doc.save_snapshot_native();
    for hwpx in [false, true] {
        let saved = reopen(&f.doc, hwpx);
        assert_layout(&saved, &f, 3);
        assert_eq!(saved.document().sections[0].paragraphs[0].text, TAB_TEXT);
        assert_eq!(saved.document().sections[0].paragraphs[1].text, TAB_TEXT);
    }
    assert!(f
        .doc
        .update_style_shapes(f.style, "{}", r#"{"tabStops":[]}"#));
    assert_eq!(body_props(&f.doc, 0)["tabStops"], json!([]));
    assert_eq!(cell_props(&f.doc, &f, 0)["tabStops"], json!([]));
    assert!(leader_fills(&f.doc, &f, None).is_empty());
    assert!(leader_fills(&f.doc, &f, Some(0)).is_empty());
    assert!((caret_x(&f.doc, &f, None, 2) - old_x).abs() < 0.2);
    f.doc.restore_snapshot_native(before).unwrap();
    assert_eq!(body_props(&f.doc, 0)["tabStops"], json!([]));
    assert!((caret_x(&f.doc, &f, None, 2) - old_x).abs() < 0.2);
    f.doc.restore_snapshot_native(applied).unwrap();
    assert_layout(&f.doc, &f, 3);
}

#[test]
fn direct_paragraph_and_char_overrides_survive_style_tab_update_and_both_formats() {
    let mut f = fixture();
    let direct = r#"{"marginLeft":1200,"tabStops":[{"position":12000,"type":0,"fill":1}]}"#;
    f.doc.apply_para_format_native(0, 1, direct).unwrap();
    f.doc
        .apply_para_format_in_cell_native(0, f.parent, f.control, 1, 0, direct)
        .unwrap();
    let color = r##"{"bold":true,"textColor":"#803090"}"##;
    f.doc.apply_char_format_native(0, 1, 2, 3, color).unwrap();
    f.doc
        .apply_char_format_in_cell_native(0, f.parent, f.control, 1, 0, 2, 3, color)
        .unwrap();
    let body_before = body_props(&f.doc, 1);
    let cell_before = cell_props(&f.doc, &f, 1);
    // 문단 여백도 2배 HWPUNIT다. 원래의 1200은 공개 조회에서 8px다.
    assert_eq!(body_before["marginLeft"], 8.0);
    assert_eq!(cell_before["marginLeft"], 8.0);
    assert!(f.doc.update_style_shapes(
        f.style,
        "{}",
        &json!({"tabStops":[{"position":STOP,"type":0,"fill":2}]}).to_string()
    ));
    assert_layout(&f.doc, &f, 2);
    assert_eq!(body_props(&f.doc, 1), body_before);
    assert_eq!(cell_props(&f.doc, &f, 1), cell_before);
    for doc in std::iter::once(&f.doc).chain([reopen(&f.doc, false), reopen(&f.doc, true)].iter()) {
        for props in [body_props(doc, 1), cell_props(doc, &f, 1)] {
            assert_eq!(props["marginLeft"], 8.0);
            assert_eq!(
                props["tabStops"],
                json!([{"position":12000,"type":0,"fill":1}])
            );
        }
        let body_char = value(&doc.get_char_properties_at_native(0, 1, 2).unwrap());
        let cell_char = value(
            &doc.get_cell_char_properties_at_native(0, f.parent, f.control, 1, 0, 2)
                .unwrap(),
        );
        for props in [body_char, cell_char] {
            assert_eq!(props["bold"], true);
            assert_eq!(props["textColor"], "#803090");
        }
    }
}

#[test]
fn character_style_and_invalid_style_do_not_replace_paragraph_tabs() {
    let mut f = fixture();
    let style = f.doc.create_style(r#"{"name":"글자 전용","type":1}"#);
    assert!(style > 0);
    let mut model = f.doc.document().clone();
    model.sections[0].paragraphs[0].style_id = style as u8;
    let Control::Table(table) = &mut model.sections[0].paragraphs[f.parent].controls[f.control]
    else {
        panic!("표 없음");
    };
    table.cells[0].paragraphs[0].style_id = style as u8;
    f.doc.set_document(model);
    let body_before = body_props(&f.doc, 0);
    let cell_before = cell_props(&f.doc, &f, 0);
    assert!(f.doc.update_style_shapes(
        style as u32,
        "{}",
        &json!({"tabStops":[{"position":STOP,"type":0,"fill":3}]}).to_string()
    ));
    assert_eq!(body_props(&f.doc, 0), body_before);
    assert_eq!(cell_props(&f.doc, &f, 0), cell_before);
    let model_before = format!("{:?}", f.doc.document());
    assert!(!f
        .doc
        .update_style_shapes(u32::MAX, "{}", r#"{"tabStops":[]}"#));
    assert_eq!(format!("{:?}", f.doc.document()), model_before);

    let broken_style = f.doc.create_style(r#"{"name":"없는 문단 모양","type":0}"#);
    let mut model = f.doc.document().clone();
    model.doc_info.styles[broken_style as usize].para_shape_id = u16::MAX;
    f.doc.set_document(model);
    let tab_count = f.doc.document().doc_info.tab_defs.len();
    let shape_count = f.doc.document().doc_info.para_shapes.len();
    assert!(f.doc.update_style_shapes(
        broken_style as u32,
        "{}",
        &json!({"tabStops":[{"position":STOP+1234,"type":0,"fill":2}]}).to_string()
    ));
    assert_eq!(
        f.doc.document().doc_info.tab_defs.len(),
        tab_count,
        "없는 문단 모양의 탭 정의를 따로 추가하지 않는다"
    );
    assert_eq!(f.doc.document().doc_info.para_shapes.len(), shape_count);
    assert_eq!(
        f.doc.document().doc_info.styles[broken_style as usize].para_shape_id,
        u16::MAX
    );
}
