#![cfg(not(target_arch = "wasm32"))]
//! 하나의 셀처럼 적용한 범위의 배경·선만 바꿔도 범위에 있던 중심선과 꺾은 대각선을 지우지 않는다.
//! 한컴 문서에서 읽은 범위 모양(중심선·꺾은선 비트 8~13)은 그대로 두고, 바꾼 값만 새 정의에 들어가야 한다.
//! 또 범위가 시작하는 셀에 대각선을 주면 한컴 12.30처럼 셀 자신의 모양에 두고 1칸 범위를 만들지 않는다.
use rhwp::model::control::Control;
use rhwp::model::style::CenterLine;
use rhwp::wasm_api::HwpDocument;
use serde_json::Value;

const LINE: &str = r##"{"type":1,"width":0,"color":"#000000"}"##;

fn table_doc() -> (HwpDocument, u32, u32) {
    let mut doc = HwpDocument::create_empty();
    let created = doc
        .create_table_ex_native(0, 0, 0, 3, 3, true, Some(&[8000; 3]), Some(&[2000; 3]))
        .expect("표 생성");
    let created: Value = serde_json::from_str(&created).expect("JSON");
    (
        doc,
        created["paraIdx"].as_u64().unwrap() as u32,
        created["controlIdx"].as_u64().unwrap() as u32,
    )
}

fn zone_bf(doc: &HwpDocument) -> rhwp::model::style::BorderFill {
    doc.document().doc_info.border_fills[zone_bf_id(doc) as usize - 1].clone()
}

fn zone_bf_id(doc: &HwpDocument) -> u16 {
    let table = doc.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .find_map(|c| match c {
            Control::Table(t) => Some(t),
            _ => None,
        })
        .expect("표");
    let zone = table
        .zones
        .iter()
        .find(|z| (z.start_row, z.start_col, z.end_row, z.end_col) == (0, 0, 1, 1))
        .expect("범위");
    zone.border_fill_id
}

/// 범위 A1:B2를 대각선과 함께 칠하고, 그 범위 정의를 `edit`로 고친(한컴 문서에서 읽은 것처럼) 문서.
fn zone_doc(edit: impl Fn(&mut rhwp::model::style::BorderFill)) -> (HwpDocument, u32, u32, u16) {
    let (mut doc, ppi, ci) = table_doc();
    let json = format!(
        r##"{{"borderLeft":{LINE},"borderRight":{LINE},"borderTop":{LINE},"borderBottom":{LINE},
        "fillType":"none","diagonalLine":1,"diagonalSlash":2,"diagonalBackSlash":2,
        "diagonalWidth":0,"diagonalColor":"#ff0000","centerLine":"NONE"}}"##
    );
    let applied: Value = serde_json::from_str(
        &doc.set_cell_zone_properties(0, ppi, ci, 0, 0, 1, 1, &json)
            .expect("범위 적용"),
    )
    .unwrap();
    let id = applied["borderFillId"].as_u64().unwrap() as u16;
    let bf = &mut doc.document_mut().doc_info.border_fills[id as usize - 1];
    edit(bf);
    bf.raw_data = None;
    (doc, ppi, ci, id)
}

/// 앱이 범위 배경만 바꿀 때 보내는 값(조회한 선과 새 면 색, 지금 정의 번호).
fn fill_change(id: u16) -> String {
    format!(
        r##"{{"borderFillId":{id},"borderLeft":{LINE},"borderRight":{LINE},"borderTop":{LINE},
        "borderBottom":{LINE},"fillType":"solid","fillColor":"#ffe08a","patternType":-1,
        "patternColor":"#000000"}}"##
    )
}

#[test]
fn zone_fill_change_keeps_bent_diagonal_bits() {
    // 비트 8~9: ／ 꺾은선, 11: ／ 180도 돌림, 10: ＼ 꺾은선.
    let bent = (2 << 8) | (1 << 11) | (1 << 10);
    let (mut doc, ppi, ci, id) = zone_doc(|bf| bf.attr |= bent);
    doc.set_cell_zone_properties(0, ppi, ci, 0, 0, 1, 1, &fill_change(id))
        .expect("배경 바꾸기");
    let bf = zone_bf(&doc);
    assert_eq!(bf.attr & 0x3f00, bent, "꺾은 대각선 비트 유지");
    assert_eq!(
        ((bf.attr >> 2) & 7, (bf.attr >> 5) & 7),
        (2, 2),
        "대각선 모양 유지"
    );
    assert_eq!(bf.fill.solid.map(|s| s.background_color), Some(0x8ae0ff));

    // 대각선 색만 바꿔도(같은 모양을 다시 보내도) 꺾은선은 그대로다.
    let json = fill_change(zone_bf_id(&doc)).replace(
        "\"fillType\":\"solid\"",
        "\"diagonalSlash\":2,\"diagonalBackSlash\":2,\"diagonalColor\":\"#0000ff\",\"fillType\":\"solid\"",
    );
    doc.set_cell_zone_properties(0, ppi, ci, 0, 0, 1, 1, &json)
        .expect("대각선 색 바꾸기");
    let bf = zone_bf(&doc);
    assert_eq!(
        bf.attr & 0x3f00,
        bent,
        "같은 모양이면 꺾은 대각선 비트 유지"
    );
    assert_eq!(bf.diagonal.color, 0xff0000);
}

#[test]
fn zone_fill_change_keeps_center_line() {
    let (mut doc, ppi, ci, id) = zone_doc(|bf| {
        bf.attr &= !(0x3f << 2);
        bf.center_line = CenterLine::Cross;
        bf.attr |= CenterLine::Cross.hwp_attr_bits();
    });
    // 앱은 조회한 중심선도 함께 보낼 수 있다. 하나의 셀처럼 적용은 중심선을 새로 주지 않고 있던 것을 둔다.
    let json = fill_change(id).replace(
        "\"fillType\":\"solid\"",
        "\"centerLine\":\"CROSS\",\"fillType\":\"solid\"",
    );
    doc.set_cell_zone_properties(0, ppi, ci, 0, 0, 1, 1, &json)
        .expect("배경 바꾸기");
    let bf = zone_bf(&doc);
    assert_eq!(bf.center_line, CenterLine::Cross, "중심선 유지");
    assert_eq!(CenterLine::from_hwp_attr(bf.attr), CenterLine::Cross);
    assert_eq!(bf.fill.solid.map(|s| s.background_color), Some(0x8ae0ff));

    // 중심선 키가 없어도 지우지 않는다.
    doc.set_cell_zone_properties(0, ppi, ci, 0, 0, 1, 1, &fill_change(zone_bf_id(&doc)))
        .expect("배경 다시 바꾸기");
    assert_eq!(zone_bf(&doc).center_line, CenterLine::Cross);

    let saved = doc.export_hwp_with_adapter().expect("HWP 저장");
    let reparsed = HwpDocument::from_bytes(&saved).expect("다시 열기");
    assert_eq!(
        zone_bf(&reparsed).center_line,
        CenterLine::Cross,
        "HWP 왕복"
    );
}

#[test]
fn origin_cell_diagonal_goes_to_cell_shape_not_one_cell_zone() {
    let (mut doc, ppi, ci, _) = zone_doc(|_| {});
    let json = format!(
        r##"{{"borderLeft":{LINE},"diagonalLine":1,"diagonalSlash":0,"diagonalBackSlash":2,
        "diagonalWidth":1,"diagonalColor":"#000000"}}"##
    );
    let result: Value = serde_json::from_str(
        &doc.set_cell_properties(0, ppi, ci, 0, &json)
            .expect("셀 대각선"),
    )
    .unwrap();
    assert_eq!(
        result["zones"].as_array().map(Vec::len),
        Some(0),
        "범위 전이 없음"
    );
    let own: Value =
        serde_json::from_str(&doc.get_cell_own_properties(0, ppi, ci, 0).unwrap()).unwrap();
    assert_eq!(own["diagonalBackSlash"].as_u64(), Some(2));
    let table_zones = zone_count(&doc);
    assert_eq!(table_zones, 1, "1칸 범위를 더하지 않는다");
}

fn zone_count(doc: &HwpDocument) -> usize {
    doc.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .find_map(|c| match c {
            Control::Table(t) => Some(t.zones.len()),
            _ => None,
        })
        .unwrap()
}
