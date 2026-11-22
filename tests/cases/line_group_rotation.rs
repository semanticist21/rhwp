//! 직선·묶음 회전과 대칭. 한/글 12.30은 개체 자리(틀 왼쪽 위)를 두고 틀을 돌린 외접 상자로
//! 바꾸며, 직선 끝점과 묶음 자식의 원래 좌표는 두고 회전·대칭을 저장 행렬에 적는다.
//! 쪽 개체 목록의 끝점·상자와 그린 모양이 같고, HWP/HWPX로 저장해 다시 열어도 같다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::model::shape::ShapeObject;
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use rhwp::renderer::PathCommand;
use serde_json::{json, Value};

const PX: f64 = 96.0 / 7200.0;
const RED: u32 = 0x0000ff;
const GREEN: u32 = 0x00ff00;

fn add(doc: &mut DocumentCore, kind: &str, x: u32, y: u32, w: u32, h: u32) -> usize {
    let result: Value = serde_json::from_str(
        &doc.create_shape_control_native(
            0,
            0,
            0,
            w,
            h,
            x,
            y,
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
    result["controlIdx"].as_u64().unwrap() as usize
}

fn set(doc: &mut DocumentCore, index: usize, props: Value) {
    doc.set_shape_properties_native(0, 0, index, &props.to_string())
        .unwrap();
}

/// 그린 직선의 (시작, 끝)과 그 노드의 회전·대칭 여부
fn drawn_line(doc: &DocumentCore) -> ([f64; 4], bool) {
    fn visit(node: &RenderNode) -> Option<([f64; 4], bool)> {
        if let RenderNodeType::Line(line) = &node.node_type {
            let t = &line.transform;
            return Some((
                [line.x1, line.y1, line.x2, line.y2],
                t.rotation != 0.0 || t.horz_flip || t.vert_flip,
            ));
        }
        node.children.iter().find_map(visit)
    }
    visit(&doc.build_page_render_tree(0).unwrap().root).expect("직선 노드")
}

/// 쪽 개체 목록의 직선 (상자, 끝점)
fn listed_line(doc: &DocumentCore) -> ([f64; 4], [f64; 4]) {
    let layout: Value =
        serde_json::from_str(&doc.get_page_control_layout_native(0).unwrap()).unwrap();
    let c = layout["controls"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "line")
        .expect("직선");
    let f = |k: &str| c[k].as_f64().unwrap();
    (
        [f("x"), f("y"), f("w"), f("h")],
        [f("x1"), f("y1"), f("x2"), f("y2")],
    )
}

fn close(label: &str, actual: &[f64], expected: &[f64]) {
    for (a, e) in actual.iter().zip(expected) {
        assert!((a - e).abs() < 0.2, "{label}: {actual:?} ≠ {expected:?}");
    }
}

fn line_props(doc: &DocumentCore, index: usize) -> Value {
    serde_json::from_str(&doc.get_shape_properties_native(0, 0, index).unwrap()).unwrap()
}

#[test]
fn rotated_and_flipped_line_keeps_position_and_lists_drawn_endpoints() {
    let mut doc = DocumentCore::new_empty();
    doc.create_blank_document_native().unwrap();
    let index = add(&mut doc, "line", 5000, 35000, 12000, 8000);
    let (frame, ends) = listed_line(&doc);
    let (left, top) = (frame[0], frame[1]);
    close(
        "처음 끝점",
        &ends,
        &[left, top, left + 12000.0 * PX, top + 8000.0 * PX],
    );

    // 오른쪽으로 90도: 틀은 8000×12000, 왼쪽 위는 제자리, 끝점은 그대로 돌아간다.
    set(&mut doc, index, json!({ "rotationAngle": 90 }));
    let props = line_props(&doc, index);
    assert_eq!(
        (props["width"].as_u64(), props["height"].as_u64()),
        (Some(8000), Some(12000))
    );
    assert_eq!(
        (props["horzOffset"].as_u64(), props["vertOffset"].as_u64()),
        (Some(5000), Some(35000))
    );
    let rotated = [left + 8000.0 * PX, top, left, top + 12000.0 * PX];
    // 좌우 대칭을 더하면 돌리기 전 틀에서 뒤집은 뒤 돌린다.
    let flipped = [left + 8000.0 * PX, top + 12000.0 * PX, left, top];
    for (label, props, expected) in [
        ("90도", json!({}), rotated),
        ("90도와 좌우 대칭", json!({ "horzFlip": true }), flipped),
    ] {
        set(&mut doc, index, props);
        let check = |stage: &str, doc: &DocumentCore| {
            let (frame, ends) = listed_line(doc);
            close(
                &format!("{label} {stage} 상자"),
                &frame,
                &[left, top, 8000.0 * PX, 12000.0 * PX],
            );
            close(&format!("{label} {stage} 목록 끝점"), &ends, &expected);
            let (drawn, transformed) = drawn_line(doc);
            close(&format!("{label} {stage} 그린 끝점"), &drawn, &expected);
            assert!(
                !transformed,
                "{label} {stage}: 끝점에 회전·대칭을 다시 걸면 안 된다"
            );
        };
        check("직후", &doc);
        check(
            "HWP",
            &DocumentCore::from_bytes(&doc.export_hwp_native().unwrap()).unwrap(),
        );
        check(
            "HWPX",
            &DocumentCore::from_bytes(&doc.export_hwpx_native().unwrap()).unwrap(),
        );
    }

    // 되돌리면 처음 틀과 끝점으로 돌아온다.
    set(
        &mut doc,
        index,
        json!({ "rotationAngle": 0, "horzFlip": false }),
    );
    let (frame, ends) = listed_line(&doc);
    close(
        "되돌림 상자",
        &frame,
        &[left, top, 12000.0 * PX, 8000.0 * PX],
    );
    close(
        "되돌림 끝점",
        &ends,
        &[left, top, left + 12000.0 * PX, top + 8000.0 * PX],
    );
}

/// 색으로 구분한 도형이 그려진 쪽 상자(px)
fn painted_boxes(doc: &DocumentCore) -> Vec<(u32, [f64; 4])> {
    fn visit(node: &RenderNode, out: &mut Vec<(u32, [f64; 4])>) {
        let (fill, points): (Option<u32>, Vec<(f64, f64)>) = match &node.node_type {
            RenderNodeType::Rectangle(r) => {
                let b = node.bbox;
                (
                    r.style.fill_color,
                    vec![(b.x, b.y), (b.x + b.width, b.y + b.height)],
                )
            }
            RenderNodeType::Path(p) => (
                p.style.fill_color,
                p.commands
                    .iter()
                    .filter_map(|c| match c {
                        PathCommand::MoveTo(x, y) | PathCommand::LineTo(x, y) => Some((*x, *y)),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => (None, Vec::new()),
        };
        if let Some(fill @ (RED | GREEN)) = fill {
            let (mut l, mut t, mut r, mut b) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for (x, y) in points {
                (l, t, r, b) = (l.min(x), t.min(y), r.max(x), b.max(y));
            }
            out.push((fill, [l, t, r - l, b - t]));
        }
        for child in &node.children {
            visit(child, out);
        }
    }
    let mut out = Vec::new();
    visit(&doc.build_page_render_tree(0).unwrap().root, &mut out);
    out.sort_by_key(|(c, _)| *c);
    out
}

fn group_index(doc: &DocumentCore) -> usize {
    doc.document().sections[0].paragraphs[0]
        .controls
        .iter()
        .position(|c| matches!(c, Control::Shape(s) if matches!(s.as_ref(), ShapeObject::Group(_))))
        .expect("묶음")
}

#[test]
fn rotated_and_flipped_group_turns_children_inside_kept_position() {
    let mut doc = DocumentCore::new_empty();
    doc.create_blank_document_native().unwrap();
    for (x, y, w, h, fill) in [
        (9000, 12000, 8000, 6000, RED),
        (21000, 24000, 6000, 4000, GREEN),
    ] {
        let ci = add(&mut doc, "rectangle", x, y, w, h);
        set(&mut doc, ci, json!({ "fillBgColor": fill }));
    }
    let targets: Vec<_> = doc.document().sections[0].paragraphs[0]
        .controls
        .iter()
        .enumerate()
        .filter(|(_, c)| matches!(c, Control::Shape(_)))
        .map(|(i, _)| (0, i))
        .collect();
    doc.group_shapes_native(0, &targets).unwrap();
    let before = painted_boxes(&doc);
    assert_eq!(before.len(), 2);
    let (left, top) = (before[0].1[0], before[0].1[1]);
    let index = group_index(&doc);
    let hu = |x: f64, y: f64, w: f64, h: f64| [left + x * PX, top + y * PX, w * PX, h * PX];

    // 묶음 틀 18000×16000. 오른쪽으로 90도 돌리면 (x, y) → (16000 − y, x), 틀은 16000×18000.
    let rotated = vec![
        (RED, hu(10000.0, 0.0, 6000.0, 8000.0)),
        (GREEN, hu(0.0, 12000.0, 4000.0, 6000.0)),
    ];
    // 돌리지 않은 묶음의 좌우 대칭은 (x, y) → (18000 − x, y)다.
    let flipped = vec![
        (RED, hu(10000.0, 0.0, 8000.0, 6000.0)),
        (GREEN, hu(0.0, 12000.0, 6000.0, 4000.0)),
    ];
    for (label, props, undo, size, expected) in [
        (
            "90도",
            json!({ "rotationAngle": 90 }),
            json!({ "rotationAngle": 0 }),
            (16000, 18000),
            rotated,
        ),
        (
            "좌우 대칭",
            json!({ "horzFlip": true }),
            json!({ "horzFlip": false }),
            (18000, 16000),
            flipped,
        ),
    ] {
        set(&mut doc, index, props.clone());
        let props_now: Value =
            serde_json::from_str(&doc.get_shape_properties_native(0, 0, index).unwrap()).unwrap();
        for key in ["rotationAngle", "horzFlip"] {
            if let Some(v) = props.get(key) {
                assert_eq!(&props_now[key], v, "{label}: 속성 {key}");
            }
        }
        let check = |stage: &str, doc: &DocumentCore| {
            let g = &doc.document().sections[0].paragraphs[0].controls[group_index(doc)];
            let Control::Shape(shape) = g else {
                unreachable!()
            };
            let common = shape.common();
            assert_eq!(
                (common.width, common.height),
                size,
                "{label} {stage}: 틀 크기"
            );
            let actual = painted_boxes(doc);
            assert_eq!(actual.len(), 2, "{label} {stage}: 자식 수");
            for ((c, a), (e_c, e)) in actual.iter().zip(&expected) {
                assert_eq!(c, e_c);
                for i in 0..4 {
                    assert!(
                        (a[i] - e[i]).abs() < 0.6,
                        "{label} {stage}: 색 {c:06x} 상자 {a:?} ≠ {e:?}"
                    );
                }
            }
        };
        check("직후", &doc);
        check(
            "HWP",
            &DocumentCore::from_bytes(&doc.export_hwp_native().unwrap()).unwrap(),
        );
        check(
            "HWPX",
            &DocumentCore::from_bytes(&doc.export_hwpx_native().unwrap()).unwrap(),
        );
        set(&mut doc, index, undo);
        let restored = painted_boxes(&doc);
        for ((_, a), (_, e)) in restored.iter().zip(&before) {
            for i in 0..4 {
                assert!((a[i] - e[i]).abs() < 0.6, "{label} 되돌림: {a:?} ≠ {e:?}");
            }
        }
    }
}
