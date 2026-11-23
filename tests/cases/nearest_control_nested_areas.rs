//! F11(앞 개체 고르기)의 주소: 같은 글자 자리의 누름틀, 표 셀·각주·머리말 안 누름틀.
//!
//! 한/글 12.30 Mac의 ⌃F11은 같은 자리의 빈 누름틀 둘을 뒤의 것부터 하나씩 고르고, 고른 누름틀에 친 글이
//! 그 누름틀에 든다. 셀·각주·머리말 안에서는 그 영역의 누름틀을 먼저 고르고, 다음에 셀이면 그 표를,
//! 각주·머리말이면 본문의 그 조판 부호 자체에 멈춘 뒤 그 앞 본문 개체로 간다. 본문에서는 표 안으로 들어가지 않는다.

use rhwp::document_core::queries::{BookmarkHeaderFooter, BookmarkPlace};
use rhwp::document_core::DocumentCore;
use serde_json::Value;

fn blank(text: &str) -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.insert_text_native(0, 0, 0, text).unwrap();
    core
}

fn json(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap()
}

fn body(para: usize) -> BookmarkPlace {
    BookmarkPlace {
        section: 0,
        para,
        cell_path: vec![],
        header_footer: None,
    }
}

fn nested(para: usize, path: Vec<(usize, usize, usize)>) -> BookmarkPlace {
    BookmarkPlace {
        cell_path: path,
        ..body(para)
    }
}

fn find(core: &DocumentCore, place: &BookmarkPlace, offset: usize, from: Option<usize>) -> Value {
    json(
        &core
            .find_nearest_control_at_native(place, offset, from, false)
            .unwrap(),
    )
}

fn field(core: &mut DocumentCore, para: usize, offset: usize, name: &str) -> usize {
    core.insert_click_here_field_at(0, para, offset, &format!("{name}안내"), "", name, true)
        .unwrap();
    let list = json(&core.get_field_list_json());
    let found = list
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == name)
        .unwrap();
    found["controlIndex"].as_u64().unwrap() as usize
}

fn values(core: &DocumentCore) -> Vec<(String, String, u64, u64)> {
    json(&core.get_field_list_json())
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["name"].as_str().unwrap().to_string(),
                f["value"].as_str().unwrap().to_string(),
                f["startPos"].as_u64().unwrap(),
                f["endPos"].as_u64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn same_place_click_here_fields_have_distinct_addresses_and_take_typing_one_by_one() {
    // "앞 뒤끝"의 글자 자리 2에 빈 누름틀 가·나를 차례로 넣으면 원시 슬롯은 가 시작·가 끝·나 시작·나 끝이다.
    let mut core = blank("앞 뒤끝");
    let a = field(&mut core, 0, 2, "가");
    let b = field(&mut core, 0, 2, "나");
    assert_ne!(a, b);
    // 문단 끝에서 F11: 나, 그 앞 형제 가, 그다음은 없다.
    let first = find(&core, &body(0), 4, None);
    assert_eq!(first["type"], "field");
    assert_eq!(first["ci"].as_u64(), Some(b as u64));
    assert_eq!(
        (first["start"].as_u64(), first["end"].as_u64()),
        (Some(2), Some(2))
    );
    let second = find(&core, &body(0), 4, Some(b));
    assert_eq!(second["ci"].as_u64(), Some(a as u64));
    assert_eq!(find(&core, &body(0), 4, Some(a))["type"], "none");

    // 고른 나를 활성으로 두면 그 자리 입력은 나에 든다.
    assert!(core.set_active_field_at(&body(0), b).unwrap());
    assert!(!core.set_active_field_at(&body(0), 0).unwrap());
    core.insert_text_native(0, 0, 2, "값").unwrap();
    let expected = vec![
        ("가".into(), "".into(), 26, 29),
        ("나".into(), "값".into(), 45, 46),
    ];
    assert_eq!(values(&core), expected);
    for bytes in [
        core.export_hwp_native().unwrap(),
        core.export_hwpx_native().unwrap(),
    ] {
        assert_eq!(values(&DocumentCore::from_bytes(&bytes).unwrap()), expected);
    }
}

#[test]
fn caret_in_table_cell_picks_cell_field_then_its_table() {
    let mut core = blank("앞");
    let body_field = field(&mut core, 0, 0, "본문");
    let table = json(&core.create_table_native(0, 0, 1, 1, 1).unwrap());
    let (tp, tc) = (
        table["paraIdx"].as_u64().unwrap() as usize,
        table["controlIdx"].as_u64().unwrap() as usize,
    );
    core.insert_text_in_cell_native(0, tp, tc, 0, 0, 0, "셀 ")
        .unwrap();
    core.insert_click_here_field_at_by_path(0, tp, &[(tc, 0, 0)], 2, "셀안내", "", "셀", true)
        .unwrap();
    let cell = nested(tp, vec![(tc, 0, 0)]);

    let found = find(&core, &cell, 3, None);
    assert_eq!(found["type"], "field");
    assert_eq!(found["areas"], serde_json::json!(["table"]));
    assert_eq!(
        found["place"],
        serde_json::json!({"section":0,"para":tp,"cellPath":[[tc,0,0]]})
    );
    assert_eq!(
        (found["start"].as_u64(), found["end"].as_u64()),
        (Some(2), Some(2))
    );
    let in_cell = found["ci"].as_u64().unwrap() as usize;

    // 셀을 다 훑으면 그 표를 고른다.
    let host = find(&core, &cell, 0, Some(in_cell));
    assert_eq!(host["type"], "table");
    assert_eq!(host["ci"].as_u64(), Some(tc as u64));
    assert_eq!(
        host["place"],
        serde_json::json!({"section":0,"para":tp,"cellPath":[]})
    );
    // 표에서는 표 앞 본문 개체로 간다.
    let before = find(&core, &body(tp), 0, Some(tc));
    assert_eq!(before["ci"].as_u64(), Some(body_field as u64));

    // 표 뒤 본문에서는 셀 안으로 들어가지 않고 표를 고른다.
    let last = core.document().sections[0].paragraphs.len() - 1;
    let after = find(&core, &body(last), usize::MAX, None);
    assert_eq!(after["type"], "table");
}

#[test]
fn caret_in_footnote_picks_note_field_then_body_before_the_note() {
    let mut core = blank("각주 뒤");
    let body_field = field(&mut core, 0, 0, "본문");
    let note = json(&core.insert_footnote_native(0, 0, 2).unwrap());
    let nc = note["controlIdx"].as_u64().unwrap() as usize;
    core.insert_text_in_footnote_native(0, 0, nc, 0, 0, "주석 ")
        .unwrap();
    core.insert_click_here_field_at_by_path(0, 0, &[(nc, 0, 0)], 3, "주석안내", "", "주석", true)
        .unwrap();
    let place = nested(0, vec![(nc, 0, 0)]);

    let found = find(&core, &place, 100, None);
    assert_eq!(found["type"], "field");
    assert_eq!(found["areas"], serde_json::json!(["footnote"]));
    assert_eq!(
        found["place"],
        serde_json::json!({"section":0,"para":0,"cellPath":[[nc,0,0]]})
    );
    let in_note = found["ci"].as_u64().unwrap() as usize;
    // 각주를 다 훑으면 본문의 그 각주 조판 부호에 멈추고, 다음에 그 앞 본문 누름틀로 간다.
    let code = find(&core, &place, 0, Some(in_note));
    assert_eq!(code["type"], "footnote");
    assert_eq!(code["ci"].as_u64(), Some(nc as u64));
    assert_eq!(code["charPos"].as_u64(), Some(2));
    assert_eq!(
        code["place"],
        serde_json::json!({"section":0,"para":0,"cellPath":[]})
    );
    let next = find(&core, &body(0), 2, Some(nc));
    assert_eq!(next["ci"].as_u64(), Some(body_field as u64));
}

#[test]
fn caret_in_header_picks_header_field_then_body_before_the_header() {
    let mut core = blank("본문");
    core.add_bookmark_native(0, 0, 0, "처음").unwrap();
    let header = json(&core.create_header_footer_native(0, true, 0).unwrap());
    let (hp, hc) = (
        header["paraIndex"].as_u64().unwrap() as usize,
        header["controlIndex"].as_u64().unwrap() as usize,
    );
    core.insert_text_in_header_footer_native(0, true, 0, 0, 0, "머리 ")
        .unwrap();
    core.insert_click_here_field_at_by_path(0, hp, &[(hc, 0, 0)], 3, "머리안내", "", "머리", true)
        .unwrap();
    let place = BookmarkPlace {
        header_footer: Some(BookmarkHeaderFooter {
            is_header: true,
            apply_to: 0,
        }),
        ..body(0)
    };

    let found = find(&core, &place, 100, None);
    assert_eq!(found["type"], "field");
    assert_eq!(
        found["place"],
        serde_json::json!({"section":0,"para":0,"cellPath":[],"headerFooter":{"isHeader":true,"applyTo":0}})
    );
    assert_eq!(found["areas"], serde_json::json!([]));
    let in_header = found["ci"].as_u64().unwrap() as usize;
    // 머리말을 다 훑으면 본문의 그 머리말 조판 부호에 멈추고, 다음에 그 앞 본문 책갈피로 간다.
    let code = find(&core, &place, 0, Some(in_header));
    assert_eq!(code["type"], "header");
    assert_eq!(code["ci"].as_u64(), Some(hc as u64));
    assert_eq!(
        code["place"],
        serde_json::json!({"section":0,"para":hp,"cellPath":[]})
    );
    let next = find(&core, &body(hp), 0, Some(hc));
    assert_eq!(next["type"], "bookmark");
}

/// 각주(또는 머리말) 첫 문단의 누름틀 범위: (컨트롤 번호, 시작, 끝).
fn area_ranges(core: &DocumentCore, para: usize, control: usize) -> Vec<(usize, usize, usize)> {
    let paragraphs = match &core.document().sections[0].paragraphs[para].controls[control] {
        rhwp::model::control::Control::Footnote(note) => &note.paragraphs,
        rhwp::model::control::Control::Header(header) => &header.paragraphs,
        _ => panic!("각주·머리말이 아니다"),
    };
    paragraphs[0]
        .field_ranges
        .iter()
        .map(|range| (range.control_idx, range.start_char_idx, range.end_char_idx))
        .collect()
}

/// 저장한 HWP·HWPX를 다시 열어도 같은 범위인지 본다.
fn assert_saved(core: &DocumentCore, para: usize, control: usize) {
    let expected = area_ranges(core, para, control);
    for bytes in [
        core.export_hwp_native().unwrap(),
        core.export_hwpx_native().unwrap(),
    ] {
        let reopened = DocumentCore::from_bytes(&bytes).unwrap();
        assert_eq!(area_ranges(&reopened, para, control), expected);
    }
}

#[test]
fn typing_in_footnote_goes_only_into_the_active_same_place_field() {
    let mut core = blank("각주 뒤");
    let note = json(&core.insert_footnote_native(0, 0, 2).unwrap());
    let nc = note["controlIdx"].as_u64().unwrap() as usize;
    core.insert_text_in_footnote_native(0, 0, nc, 0, 0, "주석 끝")
        .unwrap();
    for name in ["가", "나"] {
        core.insert_click_here_field_at_by_path(0, 0, &[(nc, 0, 0)], 3, "안내", "", name, true)
            .unwrap();
    }
    let [(a, _, _), (b, _, _)] = area_ranges(&core, 0, nc)[..] else {
        panic!("누름틀 둘이 있어야 한다");
    };

    // 고른 나를 활성으로 두면 같은 자리 입력은 나에만 든다.
    assert!(core
        .set_active_field_at(&nested(0, vec![(nc, 0, 0)]), b)
        .unwrap());
    core.insert_text_in_footnote_native(0, 0, nc, 0, 3, "값")
        .unwrap();
    assert_eq!(area_ranges(&core, 0, nc), vec![(a, 3, 3), (b, 3, 4)]);
    assert_saved(&core, 0, nc);

    // 활성 누름틀이 없으면 같은 자리의 첫 빈 누름틀 하나만 글을 품는다.
    core.clear_active_field();
    core.insert_text_in_footnote_native(0, 0, nc, 0, 3, "첫")
        .unwrap();
    assert_eq!(area_ranges(&core, 0, nc), vec![(a, 3, 4), (b, 4, 5)]);
}

#[test]
fn typing_in_header_goes_only_into_the_active_same_place_field() {
    let mut core = blank("본문");
    let header = json(&core.create_header_footer_native(0, true, 0).unwrap());
    let (hp, hc) = (
        header["paraIndex"].as_u64().unwrap() as usize,
        header["controlIndex"].as_u64().unwrap() as usize,
    );
    core.insert_text_in_header_footer_native(0, true, 0, 0, 0, "머리 끝")
        .unwrap();
    for name in ["다", "라"] {
        core.insert_click_here_field_at_by_path(0, hp, &[(hc, 0, 0)], 3, "안내", "", name, true)
            .unwrap();
    }
    let [(c, _, _), (d, _, _)] = area_ranges(&core, hp, hc)[..] else {
        panic!("누름틀 둘이 있어야 한다");
    };
    let place = BookmarkPlace {
        header_footer: Some(BookmarkHeaderFooter {
            is_header: true,
            apply_to: 0,
        }),
        ..body(0)
    };
    assert!(core.set_active_field_at(&place, d).unwrap());
    core.insert_text_in_header_footer_native(0, true, 0, 0, 3, "값")
        .unwrap();
    assert_eq!(area_ranges(&core, hp, hc), vec![(c, 3, 3), (d, 3, 4)]);
    assert_saved(&core, hp, hc);
}
