//! 한컴 12.30 곡선 그리기·다각형 편집·타원의 호·부채꼴 바꾸기.
//!
//! 기대값은 한컴 12.30 Mac이 저장한 문서에서 옮겼다.
//! - 곡선: `hp:curve`·`$cur`, 누른 점마다 꼭짓점, 구간은 모두 CURVE(Ctrl을 누른 구간만 LINE).
//! - 다각형 열기·닫기: 같은 다각형의 점 목록만 바뀐다(닫힌 것은 끝점 = 첫 점).
//! - 호·부채꼴: 타원 그대로 `hasArcPr="1" arcType="PIE|NORMAL"`, HWP 타원 속성
//!   0x0001_0102(부채꼴)·0x0001_0002(호). 시작점에서 화면 반시계 방향으로 끝점까지 그린다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::model::shape::ShapeObject;
use rhwp::model::Point;
use rhwp::wasm_api::HwpDocument;
use std::io::{Cursor, Read};

fn blank() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc
}

fn insert(doc: &mut HwpDocument, kind: &str, w: u32, h: u32, points: &[Point]) -> (usize, usize) {
    let result = doc
        .create_shape_control_native(
            0,
            0,
            0,
            w,
            h,
            5000,
            6000,
            false,
            "InFrontOfText",
            kind,
            false,
            false,
            points,
        )
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(&result).unwrap();
    (
        json["paraIdx"].as_u64().unwrap() as usize,
        json["controlIdx"].as_u64().unwrap() as usize,
    )
}

fn shape(core: &DocumentCore, para: usize, ctrl: usize) -> &ShapeObject {
    match &core.document().sections[0].paragraphs[para].controls[ctrl] {
        Control::Shape(s) => s.as_ref(),
        other => panic!("도형이 아니다: {other:?}"),
    }
}

/// 문서의 첫 도형을 찾는다(다시 연 문서의 컨트롤 순서가 바뀌어도 된다).
fn first_shape(core: &DocumentCore) -> &ShapeObject {
    core.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .find_map(|c| match c {
            Control::Shape(s) => Some(s.as_ref()),
            _ => None,
        })
        .expect("도형이 없다")
}

fn pts(list: &[(i32, i32)]) -> Vec<Point> {
    list.iter().map(|&(x, y)| Point { x, y }).collect()
}

fn xy(points: &[Point]) -> Vec<(i32, i32)> {
    points.iter().map(|p| (p.x, p.y)).collect()
}

fn section_xml(bytes: Vec<u8>) -> String {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    zip.by_name("Contents/section0.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    xml
}

#[test]
fn curve_segment_types_follow_ctrl_straight_segments() {
    let clicked = pts(&[(0, 0), (10050, 10125), (20175, 0), (30225, 10125)]);
    let mut doc = blank();
    let (para, ctrl) = insert(&mut doc, "curve", 30225, 10125, &clicked);
    // Ctrl로 그은 가운데 구간은 직선이다.
    doc.set_shape_points_native(
        0,
        para,
        ctrl,
        r#"{"points":[[0,0],[10050,10125],[20175,0],[30225,10125]],"segmentTypes":[1,0,1]}"#,
    )
    .unwrap();

    let xml = section_xml(doc.export_hwpx_native().unwrap());
    assert!(
        xml.contains(r#"<hp:seg type="LINE" x1="10050" y1="10125" x2="20175" y2="0"/>"#),
        "{xml}"
    );
    for bytes in [
        doc.export_hwp_native().unwrap(),
        doc.export_hwpx_native().unwrap(),
    ] {
        let reopened = DocumentCore::from_bytes(&bytes).unwrap();
        let ShapeObject::Curve(curve) = first_shape(&reopened) else {
            panic!("다시 연 도형이 곡선이 아니다");
        };
        assert_eq!(xy(&curve.points), xy(&clicked));
        assert_eq!(curve.segment_types, vec![1, 0, 1]);
    }
}

#[test]
fn polygon_points_move_the_frame_with_them() {
    let mut doc = blank();
    let (para, ctrl) = insert(
        &mut doc,
        "polygon",
        16875,
        12150,
        &pts(&[(0, 0), (16875, 0), (13500, 10800), (2700, 12150), (0, 0)]),
    );
    let got: serde_json::Value =
        serde_json::from_str(&doc.get_shape_points_native(0, para, ctrl).unwrap()).unwrap();
    assert_eq!(got["kind"], "polygon");
    assert_eq!(got["points"][2], serde_json::json!([13500, 10800]));

    // 한컴 '닫힌 다각형 열기'(둘째 꼭짓점에서): 그 점부터 돌고 끝점을 1mm 비켜 둔다.
    doc.set_shape_points_native(
        0,
        para,
        ctrl,
        r#"{"points":[[16875,0],[13500,10800],[2700,12150],[0,0],[17158,283]]}"#,
    )
    .unwrap();
    let ShapeObject::Polygon(poly) = shape(&doc, para, ctrl) else {
        panic!("다각형이 아니다");
    };
    assert_eq!(
        xy(&poly.points),
        vec![
            (16875, 0),
            (13500, 10800),
            (2700, 12150),
            (0, 0),
            (17158, 283)
        ]
    );
    assert_eq!((poly.common.width, poly.common.height), (17158, 12150));

    // 꼭짓점을 틀 왼쪽 위 바깥으로 끌면 틀이 그만큼 옮겨 가고 다른 점은 제자리다.
    doc.set_shape_points_native(
        0,
        para,
        ctrl,
        r#"{"points":[[16875,0],[13500,10800],[2700,12150],[-1000,-500],[17158,283]]}"#,
    )
    .unwrap();
    let ShapeObject::Polygon(poly) = shape(&doc, para, ctrl) else {
        panic!("다각형이 아니다");
    };
    assert_eq!(
        (poly.common.horizontal_offset, poly.common.vertical_offset),
        (4000, 5500)
    );
    assert_eq!(
        xy(&poly.points),
        vec![
            (17875, 500),
            (14500, 11300),
            (3700, 12650),
            (0, 0),
            (18158, 783)
        ]
    );
    let sa = &poly.drawing.shape_attr;
    assert_eq!(
        (sa.original_width, sa.current_width, poly.common.width),
        (18158, 18158, 18158)
    );

    let reopened = DocumentCore::from_bytes(&doc.export_hwp_native().unwrap()).unwrap();
    let ShapeObject::Polygon(poly) = first_shape(&reopened) else {
        panic!("다시 연 도형이 다각형이 아니다");
    };
    assert_eq!(xy(&poly.points)[3], (0, 0));
    assert_eq!(poly.common.horizontal_offset, 4000);
}

fn points_json(doc: &HwpDocument, para: usize, ctrl: usize) -> serde_json::Value {
    serde_json::from_str(&doc.get_shape_points_native(0, para, ctrl).unwrap()).unwrap()
}

fn page_points(doc: &HwpDocument, para: usize, ctrl: usize) -> Vec<(i64, i64)> {
    let c = shape(doc, para, ctrl).common();
    let (x, y) = (
        i64::from(c.horizontal_offset as i32),
        i64::from(c.vertical_offset as i32),
    );
    points_json(doc, para, ctrl)["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (x + p[0].as_i64().unwrap(), y + p[1].as_i64().unwrap()))
        .collect()
}

/// 한컴 12.30(r8g/x/hc-rot.hwpx): 돌린·뒤집은 다각형·곡선의 꼭짓점은 보이는 자리에 손잡이가 있고,
/// 고치면 보이는 자리를 점에 새겨 회전각 0·단위 행렬로 저장한다. 틀은 점 상자다.
#[test]
fn rotated_and_flipped_shapes_edit_their_visible_points() {
    let tri = pts(&[(0, 0), (8000, 0), (2000, 6000), (0, 0)]);
    let mut doc = blank();
    let (para, ctrl) = insert(&mut doc, "polygon", 8000, 6000, &tri);
    doc.set_shape_properties_native(0, para, ctrl, r#"{"horzFlip":true}"#)
        .unwrap();
    assert_eq!(
        points_json(&doc, para, ctrl)["points"],
        serde_json::json!([[8000, 0], [0, 0], [6000, 6000], [8000, 0]])
    );

    doc.set_shape_properties_native(0, para, ctrl, r#"{"horzFlip":false,"rotationAngle":90}"#)
        .unwrap();
    let c = shape(&doc, para, ctrl).common();
    let (w, h) = (i64::from(c.width), i64::from(c.height));
    // 90도 돌린 틀 가운데를 축으로 (x, y) → (가운데 + (-(y-3000), x-4000)).
    let got = points_json(&doc, para, ctrl)["points"].clone();
    let want: Vec<[i64; 2]> = [(0, 0), (8000, 0), (2000, 6000), (0, 0)]
        .iter()
        .map(|&(x, y)| [w / 2 - (y - 3000), h / 2 + (x - 4000)])
        .collect();
    assert_eq!(got, serde_json::json!(want));

    // 셋째 꼭짓점만 옮기면 다른 점은 쪽 위 제자리이고 회전이 사라진다.
    let before = page_points(&doc, para, ctrl);
    let mut moved: Vec<[i64; 2]> = serde_json::from_value(got).unwrap();
    moved[2] = [moved[2][0] - 3000, moved[2][1] + 1500];
    doc.set_shape_points_native(
        0,
        para,
        ctrl,
        &serde_json::json!({ "points": moved }).to_string(),
    )
    .unwrap();
    let after = page_points(&doc, para, ctrl);
    assert_eq!((after[0], after[1]), (before[0], before[1]));
    assert_eq!(after[2], (before[2].0 - 3000, before[2].1 + 1500));
    let sa = shape(&doc, para, ctrl).shape_attr();
    assert_eq!(
        (sa.rotation_angle, sa.horz_flip, sa.vert_flip),
        (0, false, false)
    );
    let xml = section_xml(doc.export_hwpx_native().unwrap());
    assert!(xml.contains(r#"<hp:rotationInfo angle="0""#), "{xml}");

    // 돌린 곡선도 같다.
    let (para, ctrl) = insert(
        &mut doc,
        "curve",
        9000,
        5000,
        &pts(&[(0, 5000), (3000, 0), (6000, 5000), (9000, 0)]),
    );
    doc.set_shape_properties_native(0, para, ctrl, r#"{"rotationAngle":45}"#)
        .unwrap();
    let before = page_points(&doc, para, ctrl);
    let mut moved: Vec<[i64; 2]> =
        serde_json::from_value(points_json(&doc, para, ctrl)["points"].clone()).unwrap();
    moved[3] = [moved[3][0] + 2000, moved[3][1] + 2000];
    doc.set_shape_points_native(
        0,
        para,
        ctrl,
        &serde_json::json!({ "points": moved }).to_string(),
    )
    .unwrap();
    let after = page_points(&doc, para, ctrl);
    assert_eq!(&after[..3], &before[..3]);
    assert_eq!(shape(&doc, para, ctrl).shape_attr().rotation_angle, 0);
}

/// 한컴 12.30: 가운데 기준 도형은 기준을 둔 채 위치값을 바꿔 다른 점을 제자리에 둔다
/// (왼쪽으로 17.9mm 넓히자 가로 위치 0 → -8.95mm).
#[test]
fn centered_and_right_aligned_shapes_keep_their_other_points() {
    let tri = pts(&[(0, 0), (8000, 0), (4000, 6000), (0, 0)]);
    for (align, offset) in [("Center", -500i32), ("Right", 0)] {
        let mut doc = blank();
        let (para, ctrl) = insert(&mut doc, "polygon", 8000, 6000, &tri);
        doc.set_shape_properties_native(
            0,
            para,
            ctrl,
            &format!(r#"{{"horzRelTo":"Paper","horzAlign":"{align}","horzOffset":0}}"#),
        )
        .unwrap();
        doc.set_shape_points_native(
            0,
            para,
            ctrl,
            r#"{"points":[[-1000,0],[8000,0],[4000,6000],[-1000,0]]}"#,
        )
        .unwrap();
        let c = shape(&doc, para, ctrl).common();
        assert_eq!(
            (c.horizontal_offset as i32, c.width, c.horz_align),
            (
                offset,
                9000,
                if align == "Center" {
                    rhwp::model::shape::HorzAlign::Center
                } else {
                    rhwp::model::shape::HorzAlign::Right
                }
            ),
            "{align}"
        );
    }
}

#[test]
fn ellipse_points_give_its_frame_transform() {
    let mut doc = blank();
    let (para, ctrl) = insert(&mut doc, "ellipse", 9000, 5000, &[]);
    doc.set_shape_properties_native(0, para, ctrl, r#"{"rotationAngle":30}"#)
        .unwrap();
    let got = points_json(&doc, para, ctrl);
    assert_eq!(got["kind"], "ellipse");
    assert_eq!(got["original"], serde_json::json!([9000, 5000]));
    // 원래 상자의 가운데는 틀 가운데로 간다.
    let m: Vec<f64> = serde_json::from_value(got["affine"].clone()).unwrap();
    let c = shape(&doc, para, ctrl).common();
    let center = (
        m[0] * 4500.0 + m[1] * 2500.0 + m[2],
        m[3] * 4500.0 + m[4] * 2500.0 + m[5],
    );
    assert!(
        (center.0 - f64::from(c.width) / 2.0).abs() < 1.0,
        "{center:?}"
    );
    assert!(
        (center.1 - f64::from(c.height) / 2.0).abs() < 1.0,
        "{center:?}"
    );
    assert!((m[3] - 0.5).abs() < 1e-9, "{m:?}");
    assert!(doc
        .set_shape_points_native(0, para, ctrl, r#"{"points":[[0,0],[10,10]]}"#)
        .is_err());
}
