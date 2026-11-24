//! 관계형. 각주·미주 문단의 떠 있는(글자처럼 취급하지 않은) 그림을 한/글 12.30(Mac)처럼 주석
//! 영역에 그리고, 쪽 개체 목록이 본문 문단이 뿌리인 주석 경로로 알리며, 경로로 옮기고, 붙이고,
//! 글자처럼 취급을 끄고, HWP·HWPX 저장 뒤에도 남는다.
//!
//! `note-floats-hancom.hwpx`·`.hwp` 는 한/글 12.30(Mac)이 저장한 문서다. 쪽마다 각주 하나(마지막 쪽은
//! 미주)에 14000×7000 그림이 있다: 글 앞(문단·단 0), 어울림(단 +3000), 자리차지(문단 +300),
//! 글 뒤(문단 기준 가로 +6000), 글 앞(쪽 기준 세로 +2000), 미주 어울림(단 +3000).
//! 기대 좌표는 같은 문서의 한/글 PDF(`pdftocairo -svg`, pt × 96/72)에서 쟀다.

use rhwp::model::control::Control;
use rhwp::model::shape::{HorzRelTo, VertRelTo};
use rhwp::wasm_api::HwpDocument;
use serde_json::Value;

fn path_json(path: &[(usize, usize, usize)]) -> String {
    serde_json::json!(path
        .iter()
        .map(|&(c, cell, p)| serde_json::json!({
            "controlIndex": c, "cellIndex": cell, "cellParaIndex": p
        }))
        .collect::<Vec<_>>())
    .to_string()
}

fn controls(doc: &HwpDocument, page: u32) -> Vec<Value> {
    let v: Value = serde_json::from_str(&doc.get_page_control_layout(page).unwrap()).unwrap();
    v["controls"].as_array().unwrap().clone()
}

/// 쪽 개체 목록에서 주석 경로 `[(note, 0, 0)]` 의 그림 `control`.
fn note_image(doc: &HwpDocument, page: u32, note: usize, control: usize) -> Option<Value> {
    controls(doc, page).into_iter().find(|c| {
        c["type"] == "image"
            && c["controlIdx"] == control
            && c["cellPath"]
                .as_array()
                .is_some_and(|p| p.len() == 1 && p[0]["controlIndex"] == note)
    })
}

fn xy(c: &Value) -> (f64, f64) {
    (c["x"].as_f64().unwrap(), c["y"].as_f64().unwrap())
}

/// 본문 문단 `para` 의 주석 컨트롤 번호와 그 주석 첫 문단의 그림 번호.
fn note_picture(doc: &HwpDocument, para: usize) -> (usize, usize) {
    let host = &doc.document().sections[0].paragraphs[para];
    let note = host
        .controls
        .iter()
        .position(|c| matches!(c, Control::Footnote(_) | Control::Endnote(_)))
        .unwrap();
    let paras = match &host.controls[note] {
        Control::Footnote(n) => &n.paragraphs,
        Control::Endnote(n) => &n.paragraphs,
        _ => unreachable!(),
    };
    let picture = paras[0]
        .controls
        .iter()
        .position(|c| matches!(c, Control::Picture(_)))
        .unwrap();
    (note, picture)
}

/// 쪽의 주석 글 「가나다…」 첫 줄 위(쪽 px).
fn first_text_top(doc: &HwpDocument, page: u32) -> f64 {
    let text: Value = serde_json::from_str(&doc.get_page_text_layout(page).unwrap()).unwrap();
    text["runs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["text"].as_str().is_some_and(|t| t.contains('가')))
        .map(|r| r["y"].as_f64().unwrap())
        .fold(f64::MAX, f64::min)
}

fn pt(v: f64) -> f64 {
    v * 96.0 / 72.0
}

fn hancom_docs() -> Vec<(String, HwpDocument)> {
    let mut docs = Vec::new();
    for ext in ["hwpx", "hwp"] {
        let bytes = std::fs::read(format!("tests/fixtures/note-floats-hancom.{ext}")).unwrap();
        let doc = HwpDocument::from_bytes(&bytes).unwrap();
        let hwp = HwpDocument::from_bytes(&doc.export_hwp_native().unwrap()).unwrap();
        let hwpx = HwpDocument::from_bytes(&doc.export_hwpx_native().unwrap()).unwrap();
        docs.push((ext.to_string(), doc));
        docs.push((format!("{ext}→hwp"), hwp));
        docs.push((format!("{ext}→hwpx"), hwpx));
    }
    docs
}

#[test]
fn hancom_note_floating_pictures_render_where_hancom_draws_them() {
    // (본문 문단, 쪽, 한/글 PDF 그림 x·y pt)
    let expected = [
        (0usize, 0u32, 85.077, 686.852),
        (1, 1, 115.075, 686.852),
        (2, 2, 85.077, 642.694),
        (3, 3, 145.074, 686.852),
        (4, 4, 85.077, 732.690),
        (5, 5, 115.075, 123.355),
    ];
    for (name, doc) in hancom_docs() {
        assert_eq!(doc.page_count(), 6, "{name}");
        for &(para, page, x, y) in &expected {
            let (note, picture) = note_picture(&doc, para);
            let c = note_image(&doc, page, note, picture)
                .unwrap_or_else(|| panic!("{name} 쪽 {page}: 주석 그림이 쪽 개체 목록에 없다"));
            assert_eq!(c["parentParaIdx"], para, "{name} {c}");
            let (cx, cy) = xy(&c);
            assert!(
                (cx - pt(x)).abs() < 1.5,
                "{name} 쪽 {page} x={cx} 한/글 {}",
                pt(x)
            );
            if page == 5 {
                // 미주는 본문 흐름 경로로 그린다. 미주 영역 자체의 쪽 위 자리는 이 검사 밖이고, 그림은
                // 미주 문단 첫 줄 위에 둔다. 한/글 HWP 저장본의 어울림 미주 문단은 아직 그림을 마지막
                // 저장 줄 위에 두어(본문 흐름 경로) 이 검사는 HWPX 저장본만 본다.
                if name.starts_with("hwpx") {
                    let top = first_text_top(&doc, page);
                    assert!(
                        (cy - top).abs() < 2.5,
                        "{name} 미주 그림 y={cy} 첫 줄 {top}"
                    );
                }
            } else {
                assert!(
                    (cy - pt(y)).abs() < 2.5,
                    "{name} 쪽 {page} y={cy} 한/글 {}",
                    pt(y)
                );
            }
        }
        // 자리차지 각주(쪽 2)의 글은 그림 아래에서 시작한다(한/글 「3)」 줄 위 712.65pt).
        let first = first_text_top(&doc, 2);
        assert!(
            first > pt(712.65) - 3.0 && first < pt(712.65) + 13.0,
            "{name} 자리차지 뒤 첫 줄 y={first}"
        );
    }
}

#[test]
fn note_floating_picture_moves_by_path_and_round_trips() {
    let bytes = std::fs::read("tests/fixtures/note-floats-hancom.hwpx").unwrap();
    let mut doc = HwpDocument::from_bytes(&bytes).unwrap();
    let (note, picture) = note_picture(&doc, 0);
    let path = path_json(&[(note, 0, 0)]);
    // 세로는 주석 글 첫 줄에서 잰다. 문단 기준 그림의 아래 끝까지 주석 영역이 커져 글이 함께 올라간다.
    let place = |doc: &HwpDocument| {
        let (x, y) = xy(&note_image(doc, 0, note, picture).unwrap());
        (x, y - first_text_top(doc, 0))
    };
    let before = place(&doc);
    let result = doc
        .set_cell_picture_properties_by_path(
            0,
            0,
            &path,
            picture as u32,
            r#"{"horzOffset":4500,"vertOffset":750}"#,
        )
        .unwrap();
    assert!(result.contains("\"ok\":true"), "{result}");
    let after = place(&doc);
    assert!(
        (after.0 - before.0 - 60.0).abs() < 0.6,
        "{before:?} → {after:?}"
    );
    assert!(
        (after.1 - before.1 - 10.0).abs() < 0.6,
        "{before:?} → {after:?}"
    );
    for reopened in [
        HwpDocument::from_bytes(&doc.export_hwp_native().unwrap()).unwrap(),
        HwpDocument::from_bytes(&doc.export_hwpx_native().unwrap()).unwrap(),
    ] {
        let moved = place(&reopened);
        assert!((moved.0 - after.0).abs() < 0.6 && (moved.1 - after.1).abs() < 0.6);
    }
}

fn png() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
        0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
}

/// 본문 문단 `host` 끝에 「본문」과 각주(글 「주석」)를 두고 그 각주 경로를 준다.
fn add_footnote(doc: &mut HwpDocument, host: usize) -> Vec<(usize, usize, usize)> {
    doc.insert_text_native(0, host, 0, "본문").unwrap();
    doc.insert_footnote_native(0, host, 2).unwrap();
    let ci = doc.document().sections[0].paragraphs[host]
        .controls
        .iter()
        .position(|c| matches!(c, Control::Footnote(_)))
        .unwrap();
    doc.insert_text_in_footnote_native(0, host, ci, 0, 2, "주석")
        .unwrap();
    vec![(ci, 0, 0)]
}

fn blank() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc
}

fn note_common(
    doc: &HwpDocument,
    host: usize,
    note: usize,
    control: usize,
) -> rhwp::model::shape::CommonObjAttr {
    let Control::Footnote(n) = &doc.document().sections[0].paragraphs[host].controls[note] else {
        panic!("각주");
    };
    match &n.paragraphs[0].controls[control] {
        Control::Picture(p) => p.common.clone(),
        other => panic!("그림이 아니다: {other:?}"),
    }
}

/// 한/글 12.30은 떠 있는 그림을 각주에 붙이면 떠 있는 채로 문단 세로·단 가로 기준 자리 0에 둔다.
#[test]
fn floating_picture_pasted_into_note_stays_floating_at_paragraph_origin() {
    let mut doc = blank();
    let inserted: Value = serde_json::from_str(
        &doc.insert_picture(0, 0, 0, "", &png(), 3000, 2000, 1, 1, "png", "", None, None)
            .unwrap(),
    )
    .unwrap();
    let para = inserted["paraIdx"].as_u64().unwrap() as u32;
    let control = inserted["controlIdx"].as_u64().unwrap() as u32;
    doc.set_picture_properties(
        0,
        para,
        control,
        r#"{"treatAsChar":false,"textWrap":"InFrontOfText","horzRelTo":"Paper","vertRelTo":"Paper","horzOffset":20000,"vertOffset":30000}"#,
    )
    .unwrap();
    doc.copy_control(0, para, "", control).unwrap();
    let host = doc.document().sections[0].paragraphs.len() - 1;
    let note = add_footnote(&mut doc, host);
    let pasted = doc
        .paste_internal_in_cell_by_path(0, host as u32, &path_json(&note), 4)
        .unwrap();
    assert!(pasted.contains("\"ok\":true"), "{pasted}");
    let Control::Footnote(n) = &doc.document().sections[0].paragraphs[host].controls[note[0].0]
    else {
        panic!("각주");
    };
    let picture = n.paragraphs[0]
        .controls
        .iter()
        .position(|c| matches!(c, Control::Picture(_)))
        .expect("붙인 그림");
    let left = doc.document().sections[0].section_def.page_def.margin_left as f64 / 75.0;
    for doc in [
        HwpDocument::from_bytes(&doc.export_hwp_native().unwrap()).unwrap(),
        HwpDocument::from_bytes(&doc.export_hwpx_native().unwrap()).unwrap(),
    ] {
        let common = note_common(&doc, host, note[0].0, picture);
        assert!(!common.treat_as_char);
        assert_eq!(common.vert_rel_to, VertRelTo::Para);
        assert_eq!(common.horz_rel_to, HorzRelTo::Column);
        assert_eq!((common.horizontal_offset, common.vertical_offset), (0, 0));
        assert!(common.flow_with_text);
        let c = (0..doc.page_count())
            .find_map(|p| note_image(&doc, p, note[0].0, picture))
            .expect("붙인 주석 그림이 쪽 개체 목록에 없다");
        assert_eq!(c["parentParaIdx"], host, "{c}");
        // 주석 영역 왼쪽(본문 왼쪽 여백)에 둔다.
        assert!((xy(&c).0 - left).abs() < 1.0, "{c}");
    }
}

/// 각주 그림의 글자처럼 취급을 끄면 한/글처럼 주석 영역에 떠 있는 그림으로 그리고 고를 수 있다.
#[test]
fn note_picture_turned_floating_is_drawn_and_reported() {
    let mut doc = blank();
    let note = add_footnote(&mut doc, 0);
    let inserted: Value = serde_json::from_str(
        &doc.insert_picture_in_cell(
            0,
            0,
            2,
            &path_json(&note),
            &png(),
            3000,
            1500,
            1,
            1,
            "png",
            "",
            None,
            None,
        )
        .unwrap(),
    )
    .unwrap();
    let picture = inserted["controlIdx"].as_u64().unwrap() as usize;
    doc.set_cell_picture_properties_by_path(
        0,
        0,
        &path_json(&note),
        picture as u32,
        r#"{"treatAsChar":false,"textWrap":"InFrontOfText","horzRelTo":"Column","vertRelTo":"Para","horzOffset":1500,"vertOffset":0}"#,
    )
    .unwrap();
    let left = doc.document().sections[0].section_def.page_def.margin_left as f64 / 75.0;
    for doc in [
        HwpDocument::from_bytes(&doc.export_hwp_native().unwrap()).unwrap(),
        HwpDocument::from_bytes(&doc.export_hwpx_native().unwrap()).unwrap(),
    ] {
        assert!(!note_common(&doc, 0, note[0].0, picture).treat_as_char);
        let c = note_image(&doc, 0, note[0].0, picture).expect("떠 있는 주석 그림");
        let (x, y) = xy(&c);
        assert!((x - (left + 20.0)).abs() < 1.0, "{c}");
        // 쪽 아래 주석 영역에 그린다.
        assert!(y > 900.0, "{c}");
        assert!(doc.render_page_svg_native(0).unwrap().contains("<image"));
    }
}
