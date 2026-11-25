//! 돌린 그림의 외접 상자 늘이기. 한/글 12.30은 돌린 그림을 개체 속성 너비나 Shift+→로 늘이면
//! 외접 상자의 왼쪽 위를 두고 쪽 가로로만 늘여 그림을 기울인다. 이동·회전 행렬은 그대로 두고
//! 배율 행렬에 기울임 성분을 넣는다.
//! 표본: `rotated.hwpx`(30도 돌린 그림), 한/글 12.30이 그 그림을 Shift+→ 세 번
//! (`hancom_shift_right.hwpx`, 외접 너비 +849), 너비 150mm(`hancom_width_150mm.hwpx`)로 늘여 저장한 HWPX.
//! 표 셀 안 그림: `cell_rotated.hwpx`(1×1 표 칸의 30도 돌린 떠 있는 그림), 한/글 12.30이 그 그림을
//! Shift+→ 한 번(`cell_hancom_shift_right.hwpx`, 외접 너비 +283) 늘여 저장한 HWPX.
#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;

use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::paint::{GroupKind, LayerBuilder, LayerNode, LayerNodeKind, PaintOp, RenderProfile};
use rhwp::renderer::render_tree::BoundingBox;

const HU: f64 = 96.0 / 7200.0;
/// 표본 그림의 개체 틀 왼쪽 위(종이 기준 가로 10783, 세로 26226).
const ORIGIN: (f64, f64) = (10783.0, 26226.0);

fn open(name: &str) -> DocumentCore {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/rotated_picture_stretch")
            .join(name),
    )
    .expect("fixture");
    DocumentCore::from_bytes(&bytes).expect("open")
}

fn picture_at(core: &DocumentCore) -> (usize, usize) {
    core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .find_map(|(p, para)| {
            para.controls
                .iter()
                .position(|c| matches!(c, Control::Picture(_)))
                .map(|c| (p, c))
        })
        .expect("그림")
}

/// layer 트리에서 그림 그리기 연산의 상자와 그 연산을 감싼 paint 묶음 행렬(없으면 단위 행렬).
fn image_ops(node: &LayerNode, matrix: [f64; 6], out: &mut Vec<(BoundingBox, [f64; 6])>) {
    match &node.kind {
        LayerNodeKind::Group {
            children,
            group_kind,
            ..
        } => {
            let matrix = match group_kind {
                GroupKind::Group(group) => group.paint_transform.unwrap_or(matrix),
                _ => matrix,
            };
            children.iter().for_each(|c| image_ops(c, matrix, out));
        }
        LayerNodeKind::ClipRect { child, .. } | LayerNodeKind::ClipPath { child, .. } => {
            image_ops(child, matrix, out)
        }
        LayerNodeKind::Leaf { ops } => out.extend(ops.iter().filter_map(|op| match op {
            PaintOp::Image { bbox, .. } => Some((*bbox, matrix)),
            _ => None,
        })),
    }
}

/// 그린 그림의 네 꼭짓점(왼쪽 위·오른쪽 위·오른쪽 아래·왼쪽 아래, 쪽 px).
/// 기울인 그림은 그림 상자에 그린 뒤 paint 묶음 행렬로 옮긴다.
fn drawn_corners(core: &DocumentCore) -> [(f64, f64); 4] {
    let tree = core.build_page_render_tree(0).expect("page");
    let layers = LayerBuilder::new(RenderProfile::Screen).build(&tree);
    let mut found = Vec::new();
    image_ops(&layers.root, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], &mut found);
    let [(b, m)] = found[..] else {
        panic!("그림 하나: {found:?}")
    };
    [
        (b.x, b.y),
        (b.x + b.width, b.y),
        (b.x + b.width, b.y + b.height),
        (b.x, b.y + b.height),
    ]
    .map(|(x, y)| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]))
}

/// 개체 틀 기준 꼭짓점(HU)을 쪽 px로 바꿔 그린 꼭짓점과 0.5px 안에서 맞춘다.
fn assert_corners(core: &DocumentCore, expected_hu: [(f64, f64); 4]) {
    let drawn = drawn_corners(core);
    for (got, (x, y)) in drawn.iter().zip(expected_hu) {
        let want = ((ORIGIN.0 + x) * HU, (ORIGIN.1 + y) * HU);
        assert!(
            (got.0 - want.0).abs() < 0.5 && (got.1 - want.1).abs() < 0.5,
            "그린 꼭짓점 {drawn:?}, 기대(HU) {expected_hu:?}"
        );
    }
}

/// 한/글 12.30 너비 150mm 저장본의 그림 꼭짓점(개체 틀 기준 HU). 돌린 그림을 외접 상자째
/// 42519/37963배 가로로 늘인 평행사변형이다.
const WIDTH_150MM: [(f64, f64); 4] = [
    (9525.0, 0.0),
    (42519.0, 17008.0),
    (32994.0, 31737.0),
    (0.0, 14729.0),
];

#[test]
fn rotated_picture_stretch_hancom_files_draw_skewed() {
    assert_corners(&open("hancom_width_150mm.hwpx"), WIDTH_150MM);
    // Shift+→ 세 번 저장본은 한/글이 키마다 행렬을 다시 나눠 기울임이 조금 다르다.
    assert_corners(
        &open("hancom_shift_right.hwpx"),
        [
            (8589.0, 0.0),
            (38812.0, 16973.0),
            (30223.0, 31737.0),
            (0.0, 14764.0),
        ],
    );
}

#[test]
fn rotated_picture_stretch_width_matches_hancom_and_survives_reopen() {
    let mut core = open("rotated.hwpx");
    let (para, ctrl) = picture_at(&core);
    core.set_picture_properties_native(0, para, ctrl, r#"{"width":42519}"#)
        .expect("width");
    let props = core
        .get_picture_properties_native(0, para, ctrl)
        .expect("props");
    assert!(
        props.contains("\"width\":42519,") && props.contains("\"height\":31737,"),
        "외접 상자 너비만 바뀐다: {props}"
    );
    assert_corners(&core, WIDTH_150MM);

    let hwpx = core.export_hwpx_native().expect("hwpx");
    let reopened = DocumentCore::from_bytes(&hwpx).expect("reopen hwpx");
    assert_corners(&reopened, WIDTH_150MM);
    let Control::Picture(pic) = &reopened.document().sections[0].paragraphs[para].controls[ctrl]
    else {
        panic!("그림")
    };
    // 한/글 저장본과 같은 현재 크기·회전 중심·위치다.
    assert_eq!(
        (
            pic.shape_attr.current_width,
            pic.shape_attr.current_height,
            pic.shape_attr.rotation_center.x,
            pic.common.horizontal_offset,
            pic.common.vertical_offset,
        ),
        (37077, 17518, 21259, 10783, 26226)
    );

    let hwp = core.export_hwp_native().expect("hwp");
    assert_corners(
        &DocumentCore::from_bytes(&hwp).expect("reopen hwp"),
        WIDTH_150MM,
    );
}

#[test]
fn rotated_picture_stretch_keyboard_steps_keep_top_left() {
    // Shift+→ 한 번은 외접 상자를 283(1mm)씩 넓힌다. 세 번이면 한/글처럼 +849, 높이는 그대로다.
    let mut core = open("rotated.hwpx");
    let (para, ctrl) = picture_at(&core);
    for width in [38246, 38529, 38812] {
        core.set_picture_properties_native(0, para, ctrl, &format!("{{\"width\":{width}}}"))
            .expect("width");
    }
    let props = core
        .get_picture_properties_native(0, para, ctrl)
        .expect("props");
    assert!(
        props.contains("\"width\":38812,") && props.contains("\"height\":31737,"),
        "{props}"
    );
    let k = 38812.0 / 37963.0;
    assert_corners(
        &core,
        [
            (8504.0 * k, 0.0),
            (38812.0, 17008.0),
            (29459.0 * k, 31737.0),
            (0.0, 14729.0),
        ],
    );
}

/// 첫 표 첫 칸 첫 문단의 그림.
fn cell_picture(core: &DocumentCore) -> &rhwp::model::image::Picture {
    core.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|para| &para.controls)
        .find_map(|c| match c {
            Control::Table(t) => t.cells[0].paragraphs[0]
                .controls
                .iter()
                .find_map(|c| match c {
                    Control::Picture(p) => Some(p),
                    _ => None,
                }),
            _ => None,
        })
        .expect("셀 그림")
}

/// 크기·현재 크기·회전 중심과 그리기 행렬(배율·기울임·이동).
fn frame(pic: &rhwp::model::image::Picture) -> (u32, u32, u32, u32, i32, [f64; 6]) {
    let sa = &pic.shape_attr;
    (
        pic.common.width,
        pic.common.height,
        sa.current_width,
        sa.current_height,
        sa.rotation_center.x,
        [
            sa.render_sx,
            sa.render_b,
            sa.render_tx,
            sa.render_c,
            sa.render_sy,
            sa.render_ty,
        ],
    )
}

fn assert_same_frame(got: &rhwp::model::image::Picture, want: &rhwp::model::image::Picture) {
    let (g, w) = (frame(got), frame(want));
    assert_eq!(
        (g.0, g.1, g.2, g.3, g.4),
        (w.0, w.1, w.2, w.3, w.4),
        "{g:?} / {w:?}"
    );
    for (a, b) in g.5.iter().zip(w.5) {
        assert!((a - b).abs() < 0.5, "그리기 행렬 {g:?} / {w:?}");
    }
}

#[test]
fn rotated_picture_stretch_in_table_cell_matches_hancom() {
    // 한/글 12.30은 표 셀 안 돌린 그림도 본문처럼 외접 상자째 가로로만 늘여 기울인다.
    let hancom = open("cell_hancom_shift_right.hwpx");
    let mut core = open("cell_rotated.hwpx");
    let (para, _) = core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .find_map(|(p, para)| {
            para.controls
                .iter()
                .position(|c| matches!(c, Control::Table(_)))
                .map(|c| (p, c))
        })
        .expect("표");
    let path = r#"[{"controlIndex":0,"cellIndex":0,"cellParaIndex":0}]"#;
    core.set_cell_picture_properties_by_path_native(0, para, path, 0, r#"{"width":11077}"#)
        .expect("width");
    assert_same_frame(cell_picture(&core), cell_picture(&hancom));
    for bytes in [
        core.export_hwpx_native().expect("hwpx"),
        core.export_hwp_native().expect("hwp"),
    ] {
        let reopened = DocumentCore::from_bytes(&bytes).expect("reopen");
        assert_same_frame(cell_picture(&reopened), cell_picture(&hancom));
    }
}

#[test]
fn rotated_picture_stretch_in_header_matches_body() {
    // 머리말 바로 안 돌린 그림도 같은 외접 상자 늘이기다(표 셀 그림을 머리말로 옮겨 견준다).
    let hancom = open("cell_hancom_shift_right.hwpx");
    let mut core = open("cell_rotated.hwpx");
    core.create_header_footer_native(0, true, 0)
        .expect("header");
    let picture = Control::Picture(Box::new(cell_picture(&core).clone()));
    let (outer_para, outer_ctrl) = core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .find_map(|(p, para)| {
            para.controls
                .iter()
                .position(|c| matches!(c, Control::Header(_)))
                .map(|c| (p, c))
        })
        .expect("머리말");
    let Control::Header(header) =
        &mut core.document_mut().sections[0].paragraphs[outer_para].controls[outer_ctrl]
    else {
        panic!("머리말")
    };
    let inner = &mut header.paragraphs[0];
    inner.controls.push(picture);
    inner.ctrl_data_records.push(None);
    let inner_ctrl = inner.controls.len() - 1;
    core.set_header_footer_picture_properties_native(
        0,
        outer_para,
        outer_ctrl,
        0,
        inner_ctrl,
        r#"{"width":11077}"#,
    )
    .expect("width");
    let Control::Header(header) =
        &core.document().sections[0].paragraphs[outer_para].controls[outer_ctrl]
    else {
        panic!("머리말")
    };
    let Control::Picture(pic) = &header.paragraphs[0].controls[inner_ctrl] else {
        panic!("그림")
    };
    assert_same_frame(pic, cell_picture(&hancom));
}
