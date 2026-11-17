//! 관계형. 쪽 테두리/배경의 홀짝 항목, 첫 쪽 조건, 테두리 채울 영역.
//! 한/글 12.30 Mac에서 HWPX 세 항목(BOTH·EVEN·ODD)과 visibility, fillArea를 바꿔 연 세 쪽
//! 문서를 PDF로 저장해 확인했다: 짝수·홀수 항목에 선이나 채우기가 있으면 그 쪽에 그것을,
//! 없으면 양쪽 항목을 그린다. `HIDE_FIRST`는 구역 첫 쪽에서만 감추고 `SHOW_FIRST`는 첫 쪽에만
//! 그린다. `fillArea="BORDER"`는 테두리 선 안쪽만 칠한다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::wasm_api::HwpDocument;

const BORDER: &str = r##"{"type":1,"width":3,"color":"#ff0000"}"##;
const NONE: &str = r##"{"type":0,"width":0,"color":"#000000"}"##;

fn three_pages() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, "첫째 쪽").unwrap();
    doc.insert_page_break_native(0, 0, 4).unwrap();
    doc.insert_text_native(0, 1, 0, "둘째 쪽").unwrap();
    doc.insert_page_break_native(0, 1, 4).unwrap();
    doc.insert_text_native(0, 2, 0, "셋째 쪽").unwrap();
    assert_eq!(doc.page_count(), 3);
    doc
}

fn border_json(line: &str, fill: bool, extra: &str) -> String {
    let fill = if fill {
        r##""fillType":"solid","fillColor":"#ffff00""##
    } else {
        r#""fillType":"none""#
    };
    format!(
        r#"{{"borderLeft":{line},"borderRight":{line},"borderTop":{line},"borderBottom":{line},{fill}{extra}}}"#
    )
}

/// 쪽마다 (빨간 테두리, 노란 배경) 여부.
fn drawn(doc: &HwpDocument) -> Vec<(bool, bool)> {
    (0..doc.page_count())
        .map(|page| {
            let svg = doc.render_page_svg_native(page).unwrap();
            (
                svg.contains(r##"stroke="#ff0000""##),
                svg.contains(r##"fill="#ffff00""##),
            )
        })
        .collect()
}

#[test]
fn first_page_conditions_follow_hancom() {
    let mut doc = three_pages();
    let all = border_json(BORDER, true, "");
    doc.set_page_border_fill_native(0, &all).unwrap();
    assert_eq!(drawn(&doc), vec![(true, true); 3]);

    let except_first = border_json(
        BORDER,
        true,
        r#","borderPages":"exceptFirst","fillPages":"exceptFirst""#,
    );
    doc.set_page_border_fill_native(0, &except_first).unwrap();
    assert_eq!(
        drawn(&doc),
        vec![(false, false), (true, true), (true, true)],
        "첫 쪽 제외"
    );

    let first_only = border_json(
        BORDER,
        true,
        r#","borderPages":"firstOnly","fillPages":"all""#,
    );
    doc.set_page_border_fill_native(0, &first_only).unwrap();
    assert_eq!(
        drawn(&doc),
        vec![(true, true), (false, true), (false, true)],
        "테두리는 첫 쪽만, 배경은 모두"
    );
    let info = doc.get_page_border_fill_native(0).unwrap();
    assert!(info.contains(r#""borderPages":"firstOnly""#), "{info}");
    assert!(info.contains(r#""fillPages":"all""#), "{info}");

    for bytes in [
        doc.export_hwpx_native().unwrap(),
        doc.export_hwp_native().unwrap(),
    ] {
        let mut reopened = HwpDocument::from_bytes(&bytes).unwrap();
        assert_eq!(
            drawn(&reopened),
            vec![(true, true), (false, true), (false, true)]
        );
        reopened
            .set_page_border_fill_native(0, &except_first)
            .unwrap();
        let again = HwpDocument::from_bytes(&reopened.export_hwpx_native().unwrap()).unwrap();
        assert_eq!(
            drawn(&again),
            vec![(false, false), (true, true), (true, true)],
            "HWPX HIDE_FIRST 왕복"
        );
    }
}

#[test]
fn odd_and_even_entries_draw_on_their_pages() {
    let mut doc = three_pages();
    doc.set_page_border_fill_native(0, &border_json(NONE, false, ""))
        .unwrap();
    doc.set_page_border_fill_native(0, &border_json(BORDER, true, r#","pageType":"odd""#))
        .unwrap();
    assert_eq!(
        drawn(&doc),
        vec![(true, true), (false, false), (true, true)],
        "홀수 쪽만"
    );
    let info = doc.get_page_border_fill_native(0).unwrap();
    assert!(info.contains(r#""odd":{"attr""#), "{info}");

    for bytes in [
        doc.export_hwpx_native().unwrap(),
        doc.export_hwp_native().unwrap(),
    ] {
        let reopened = HwpDocument::from_bytes(&bytes).unwrap();
        assert_eq!(
            drawn(&reopened),
            vec![(true, true), (false, false), (true, true)]
        );
    }

    // 양쪽을 바꾸면 따로 고친 홀수 항목은 남고, 양쪽을 복제한 짝수 항목은 따라온다.
    let blue = r##"{"type":1,"width":3,"color":"#0000ff"}"##;
    doc.set_page_border_fill_native(0, &border_json(blue, false, ""))
        .unwrap();
    let pages: Vec<String> = (0..3)
        .map(|page| doc.render_page_svg_native(page).unwrap())
        .collect();
    assert!(pages[0].contains(r##"stroke="#ff0000""##));
    assert!(pages[1].contains(r##"stroke="#0000ff""##));
    assert!(pages[2].contains(r##"stroke="#ff0000""##));
}

#[test]
fn border_fill_area_paints_inside_the_border() {
    let mut doc = three_pages();
    doc.set_page_border_fill_native(0, &border_json(BORDER, true, r#","fillArea":"border""#))
        .unwrap();
    let svg = doc.render_page_svg_native(0).unwrap();
    let rect = svg
        .split("<rect")
        .find(|chunk| chunk.contains(r##"fill="#ffff00""##))
        .expect("배경 사각형");
    assert!(
        !rect.contains(r#"x="0" y="0""#),
        "종이 전체를 칠했다: {rect}"
    );
    let line = svg
        .split("<line")
        .find(|chunk| chunk.contains(r##"stroke="#ff0000""##))
        .expect("테두리 선");
    let attr = |chunk: &str, name: &str| -> f64 {
        let at = &chunk[chunk.find(&format!(" {name}=\"")).unwrap() + name.len() + 3..];
        at[..at.find('"').unwrap()].parse().unwrap()
    };
    assert!(
        (attr(rect, "x") - attr(line, "x1")).abs() < 0.5,
        "배경 왼쪽이 테두리 선과 같아야 한다"
    );
    let info = doc.get_page_border_fill_native(0).unwrap();
    assert!(info.contains(r#""fillArea":"border""#), "{info}");
}

/// 빨간 테두리 선의 가장 위·아래 y.
fn border_top_bottom(doc: &HwpDocument) -> (f64, f64) {
    let svg = doc.render_page_svg_native(0).unwrap();
    let ys: Vec<f64> = svg
        .split("<line")
        .filter(|chunk| chunk.contains(r##"stroke="#ff0000""##))
        .flat_map(|chunk| {
            ["y1", "y2"].map(|name| {
                let at = &chunk[chunk.find(&format!(" {name}=\"")).unwrap() + name.len() + 3..];
                at[..at.find('"').unwrap()].parse::<f64>().unwrap()
            })
        })
        .collect();
    (
        ys.iter().cloned().fold(f64::MAX, f64::min),
        ys.iter().cloned().fold(f64::MIN, f64::max),
    )
}

#[test]
fn header_and_footer_inside_widen_the_page_based_border() {
    let mut doc = three_pages();
    doc.set_page_border_fill_native(0, &border_json(BORDER, false, r#","basis":"page""#))
        .unwrap();
    let (top, bottom) = border_top_bottom(&doc);
    doc.set_page_border_fill_native(
        0,
        &border_json(
            BORDER,
            false,
            r#","basis":"page","headerInside":true,"footerInside":true"#,
        ),
    )
    .unwrap();
    let (wide_top, wide_bottom) = border_top_bottom(&doc);
    let page = &doc.document().sections[0].section_def.page_def;
    let header = rhwp::renderer::hwpunit_to_px(page.margin_header as i32, 96.0);
    let footer = rhwp::renderer::hwpunit_to_px(page.margin_footer as i32, 96.0);
    assert!((top - wide_top - header).abs() < 1.0, "{top} → {wide_top}");
    assert!(
        (wide_bottom - bottom - footer).abs() < 1.0,
        "{bottom} → {wide_bottom}"
    );
    let info = doc.get_page_border_fill_native(0).unwrap();
    assert!(info.contains(r#""headerInside":true"#), "{info}");
}

/// 한/글 12.30 편집 용지 배경 탭의 색(면 색·무늬)·그러데이션·그림 채우기.
/// 조회는 세부 값을 알리고, 그린 SVG와 두 형식 왕복이 그 값을 지킨다.
#[test]
fn pattern_gradient_and_picture_backgrounds_round_trip() {
    const PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x02, 0x00, 0x00, 0x00, 0xfd,
        0xd4, 0x9a, 0x73, 0x00, 0x00, 0x00, 0x14, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8,
        0xcf, 0xc0, 0xc0, 0x00, 0xc2, 0x0c, 0xff, 0xff, 0xff, 0x67, 0x00, 0x00, 0x1e, 0xef, 0x04,
        0xfc, 0xa3, 0xc8, 0xb4, 0xf7, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42,
        0x60, 0x82,
    ];
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    let image = doc.register_image_data(PNG, "png");
    let fills = [
        r##"{"borderFillId":1,"fillType":"solid","fillColor":"#ffff99","patternType":5,"patternColor":"#ff0000"}"##.to_string(),
        r##"{"borderFillId":1,"fillType":"gradient","gradientType":3,"gradientAngle":45,"gradientCenterX":50,"gradientCenterY":50,"gradientBlur":255,"gradientStepCenter":50,"gradientStartColor":"#ff0000","gradientEndColor":"#0000ff"}"##.to_string(),
        format!(
            r#"{{"borderFillId":1,"fillType":"image","imageBinDataId":{image},"imageFillMode":"tileAll","imageEffect":1,"imageBrightness":0,"imageContrast":0}}"#
        ),
    ];
    let expect = [
        [r#""patternType":5"#, r##""patternColor":"#ff0000""##],
        [r#""gradientType":3"#, r##""gradientEndColor":"#0000ff""##],
        [r#""imageFillMode":"tileAll""#, r#""imageEffect":1"#],
    ];
    let check = |doc: &HwpDocument, label: &str| {
        let info = doc.get_page_border_fill_native(0).unwrap();
        let entry = &info[..info.find(r#""hideBorder""#).unwrap()];
        let svg = doc.render_page_svg_native(0).unwrap();
        (entry.to_string(), svg, label.to_string())
    };
    for (fill, keys) in fills.iter().zip(expect) {
        doc.set_page_border_fill_native(0, fill).unwrap();
        let mut docs = vec![check(&doc, "편집")];
        for bytes in [
            doc.export_hwpx_native().unwrap(),
            doc.export_hwp_native().unwrap(),
        ] {
            docs.push(check(&HwpDocument::from_bytes(&bytes).unwrap(), "왕복"));
        }
        for (entry, svg, label) in docs {
            for key in keys {
                assert!(entry.contains(key), "{label} {key}: {entry}");
            }
            if fill.contains("patternType") {
                // 면 색 위에 빨간 무늬를 칠한다.
                assert!(svg.contains("<pattern"), "{label} 무늬: {svg:.400}");
                assert!(svg.contains(r##"stroke="#ff0000""##), "{label} 무늬 색");
            }
            if fill.contains("gradientType") {
                // 원뿔형은 기울임(45°, 오른쪽 위) 쪽이 시작 색(빨강), 반대쪽이 끝 색(파랑)인 부채꼴이다.
                let wedges: Vec<&str> = svg.split("<path d=\"M").skip(1).take(360).collect();
                assert_eq!(wedges.len(), 360, "{label} 부채꼴");
                let fill = |wedge: &str| {
                    let at = &wedge[wedge.find("fill=\"#").unwrap() + 7..];
                    u32::from_str_radix(&at[..6], 16).unwrap()
                };
                assert!(
                    fill(wedges[45]) >> 16 > 0xf0 && fill(wedges[45]) & 0xff < 0x10,
                    "{label} 시작: {}",
                    wedges[45]
                );
                assert!(
                    fill(wedges[225]) >> 16 < 0x10 && fill(wedges[225]) & 0xff > 0xf0,
                    "{label} 끝: {}",
                    wedges[225]
                );
                assert!(
                    fill(wedges[135]) >> 16 > 0x60 && fill(wedges[135]) & 0xff > 0x60,
                    "{label} 가운데: {}",
                    wedges[135]
                );
            }
        }
    }
}
