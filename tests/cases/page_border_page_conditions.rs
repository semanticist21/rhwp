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
