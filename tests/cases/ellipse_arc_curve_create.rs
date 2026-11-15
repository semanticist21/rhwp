//! 한컴 12.30 곡선 그리기와 타원의 호·부채꼴 바꾸기.
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
fn curve_is_created_from_clicked_points_and_round_trips() {
    // 한컴 저장본(r8d-shapes.hwpx) 곡선의 점 넷.
    let clicked = pts(&[(0, 0), (10050, 10125), (20175, 0), (30225, 10125)]);
    let mut doc = blank();
    let (para, ctrl) = insert(&mut doc, "curve", 30225, 10125, &clicked);
    let ShapeObject::Curve(curve) = shape(&doc, para, ctrl) else {
        panic!("곡선이 아니다: {:?}", shape(&doc, para, ctrl));
    };
    assert_eq!(xy(&curve.points), xy(&clicked));
    assert_eq!(curve.segment_types, vec![1, 1, 1]);
    assert_eq!(curve.common.description, "곡선입니다.");

    for bytes in [
        doc.export_hwp_native().unwrap(),
        doc.export_hwpx_native().unwrap(),
    ] {
        let reopened = DocumentCore::from_bytes(&bytes).unwrap();
        let ShapeObject::Curve(curve) = first_shape(&reopened) else {
            panic!("다시 연 도형이 곡선이 아니다");
        };
        assert_eq!(xy(&curve.points), xy(&clicked));
        assert_eq!(curve.segment_types, vec![1, 1, 1]);
    }
}

#[test]
fn ellipse_becomes_pie_and_round_trips_like_hancom() {
    let mut doc = blank();
    let (para, ctrl) = insert(&mut doc, "ellipse", 16800, 10724, &[]);
    // 오른쪽(0°)에서 아래(90°)로: 한컴은 반시계로 위·왼쪽을 지나 270°를 그린다.
    doc.set_shape_properties_native(
        0,
        para,
        ctrl,
        r#"{"ellipseArcType":"pie","ellipseArcStart":0,"ellipseArcEnd":90}"#,
    )
    .unwrap();
    let ShapeObject::Ellipse(ellipse) = shape(&doc, para, ctrl) else {
        panic!("타원이 아니다");
    };
    assert_eq!(ellipse.attr, 0x0001_0102);
    assert_eq!((ellipse.start1.x, ellipse.start1.y), (16800, 5362));
    assert_eq!((ellipse.end1.x, ellipse.end1.y), (8400, 10724));

    let props = doc.get_shape_properties_native(0, para, ctrl).unwrap();
    assert!(props.contains(r#""ellipseArcType":"pie""#), "{props}");
    assert!(props.contains(r#""ellipseArcEnd":90"#), "{props}");

    // 그리기: 타원 노드 대신 경로가 위 끝(y≈6000 HU)까지 올라간다.
    let svg = doc.render_page_svg_native(0).unwrap();
    assert!(!svg.contains("<ellipse"), "{svg}");
    let path = svg
        .split("<path")
        .find(|p| p.contains(" C"))
        .expect("호 경로가 없다");
    let d = path
        .split("d=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let numbers: Vec<f64> = d
        .split(|c: char| c.is_ascii_alphabetic() || c == ' ' || c == ',')
        .filter_map(|t| t.parse::<f64>().ok())
        .collect();
    let top_px = 6000.0 * 96.0 / 7200.0;
    let ys: Vec<f64> = numbers.iter().skip(1).step_by(2).cloned().collect();
    let min_y = ys.iter().cloned().fold(f64::MAX, f64::min);
    assert!(
        (min_y - top_px).abs() < 1.5,
        "위 끝 {min_y} ≠ {top_px}: {path}"
    );

    let xml = section_xml(doc.export_hwpx_native().unwrap());
    assert!(
        xml.contains(r#"intervalDirty="0" hasArcPr="1" arcType="PIE""#),
        "{xml}"
    );
    for bytes in [
        doc.export_hwp_native().unwrap(),
        doc.export_hwpx_native().unwrap(),
    ] {
        let reopened = DocumentCore::from_bytes(&bytes).unwrap();
        let ShapeObject::Ellipse(ellipse) = first_shape(&reopened) else {
            panic!("다시 연 도형이 타원이 아니다");
        };
        assert_eq!(ellipse.attr, 0x0001_0102);
        assert_eq!((ellipse.start1.x, ellipse.end1.y), (16800, 10724));
    }

    // 두 점을 겹치면 한컴처럼 온전한 타원으로 되돌린다.
    doc.set_shape_properties_native(0, para, ctrl, r#"{"ellipseArcType":"none"}"#)
        .unwrap();
    let ShapeObject::Ellipse(ellipse) = shape(&doc, para, ctrl) else {
        panic!("타원이 아니다");
    };
    assert_eq!(ellipse.attr, 0);
}

#[test]
fn hancom_arc_ellipse_reads_from_hwpx() {
    // 한컴 저장본의 호(NORMAL) 타원 속성을 그대로 읽는다.
    let mut doc = blank();
    insert(&mut doc, "ellipse", 16876, 10724, &[]);
    let xml = section_xml(doc.export_hwpx_native().unwrap());
    let open = xml.find("<hp:ellipse ").unwrap();
    let close = open + xml[open..].find('>').unwrap();
    let head = xml[open..close].to_string();
    let edited_head = format!(
        "{} intervalDirty=\"0\" hasArcPr=\"1\" arcType=\"NORMAL\"",
        head.split(" intervalDirty").next().unwrap()
    );
    let mut edited = xml.replacen(&head, &edited_head, 1);
    for (tag, x, y) in [("start1", 20926, 6675), ("end1", 8438, 0)] {
        let at = edited.find(&format!("<hc:{tag} ")).unwrap();
        let end = at + edited[at..].find("/>").unwrap();
        edited.replace_range(at..end, &format!(r#"<hc:{tag} x="{x}" y="{y}""#));
    }

    let bytes = doc.export_hwpx_native().unwrap();
    let mut input = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..input.len() {
        let mut entry = input.by_index(i).unwrap();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        let name = entry.name().expect("HWPX ZIP 엔트리 이름").to_string();
        if name == "Contents/section0.xml" {
            data = edited.clone().into_bytes();
        }
        output
            .start_file(
                name,
                zip::write::SimpleFileOptions::default().compression_method(entry.compression()),
            )
            .unwrap();
        std::io::Write::write_all(&mut output, &data).unwrap();
    }
    let core = DocumentCore::from_bytes(&output.finish().unwrap().into_inner()).unwrap();
    let ShapeObject::Ellipse(ellipse) = first_shape(&core) else {
        panic!("타원이 아니다");
    };
    assert_eq!(ellipse.attr, 0x0001_0002);
    let reopened = DocumentCore::from_bytes(&core.export_hwp_native().unwrap()).unwrap();
    let ShapeObject::Ellipse(ellipse) = first_shape(&reopened) else {
        panic!("타원이 아니다");
    };
    assert_eq!(ellipse.attr, 0x0001_0002);
}
