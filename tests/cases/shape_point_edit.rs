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

    // 회전한 도형은 거절한다(점 좌표계가 틀과 다르다).
    doc.set_shape_properties_native(0, para, ctrl, r#"{"rotationAngle":30}"#)
        .unwrap();
    assert!(doc.get_shape_points_native(0, para, ctrl).is_err());
    assert!(doc
        .set_shape_points_native(0, para, ctrl, r#"{"points":[[0,0],[10,10]]}"#)
        .is_err());
}
