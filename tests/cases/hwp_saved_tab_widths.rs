//! 관계형. 친 탭을 HWP로 저장하면 조판한 탭 폭과 종류를 탭 확장 자료에 적는다. 한컴 12.30은 HWP의
//! 탭 폭을 다시 계산하지 않고 저장된 폭을 그대로 써, 폭 0 자리표로 저장한 문서는 탭 뒤 글이 모두
//! 어긋났다(왼쪽 탭 포함). 기준: 저장 전 캐럿 간격 = 저장된 폭, 다시 연 문서의 캐럿 = 저장 전 캐럿.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::DocumentCore;
use rhwp::model::paragraph::Paragraph;

/// TabItem.position은 문단 여백처럼 2배 HWPUNIT이다.
fn tab_pos(mm: f64) -> u32 {
    (mm * 7200.0 * 2.0 / 25.4).round() as u32
}

fn tabs_json(kind: u8) -> String {
    format!(
        r#"{{"tabStops":[{{"position":{},"type":{kind},"fill":0}},{{"position":{},"type":{kind},"fill":0}}]}}"#,
        tab_pos(40.0),
        tab_pos(80.0)
    )
}

fn x_of(json: String) -> f64 {
    let rect: serde_json::Value = serde_json::from_str(&json).unwrap();
    rect["x"].as_f64().unwrap()
}

fn tab_offsets(text: &str) -> Vec<usize> {
    text.chars()
        .enumerate()
        .filter(|(_, ch)| *ch == '\t')
        .map(|(i, _)| i)
        .collect()
}

/// 저장된 탭마다 (폭 HWPUNIT, 종류 상위 바이트).
fn stored_tabs(para: &Paragraph) -> Vec<(u32, u16)> {
    para.tab_extended
        .iter()
        .map(|ext| (u32::from(ext[0]) | (u32::from(ext[1]) << 16), ext[2] >> 8))
        .collect()
}

fn check_paragraph(
    what: &str,
    text: &str,
    stored: &[(u32, u16)],
    kind: u16,
    before: &dyn Fn(usize) -> f64,
    after: &dyn Fn(usize) -> f64,
) {
    let tabs = tab_offsets(text);
    assert_eq!(stored.len(), tabs.len(), "{what}: 탭마다 확장 자료");
    for (&(width, stored_kind), &at) in stored.iter().zip(&tabs) {
        let gap = (before(at + 1) - before(at)) * 75.0;
        assert!(
            (f64::from(width) - gap).abs() <= 45.0,
            "{what}: 탭 {at} 폭 {width} != 캐럿 간격 {gap}"
        );
        assert!(width > 0, "{what}: 탭 {at} 폭은 0이 아니다");
        assert_eq!(stored_kind, kind, "{what}: 탭 {at} 종류");
    }
    for offset in 0..=text.chars().count() {
        assert!(
            (after(offset) - before(offset)).abs() < 0.6,
            "{what}: 다시 연 문서의 캐럿 {offset}"
        );
    }
}

#[test]
fn hwp_save_writes_laid_out_tab_widths_and_kinds() {
    // (탭 종류(None은 기본 탭), 글). 저장 종류 바이트는 TabDef 종류 + 1이다.
    // 기본 탭 문단을 맨 앞에 둔다. 나눈 문단은 앞 문단의 탭 정지를 이어받는다.
    let rows: [(Option<u8>, &str); 5] = [
        (None, "다\t라\t마"),
        (Some(0), "나\tGHI"),
        (Some(1), "가\tABC\tDEF"),
        (Some(2), "가\tABC\tDEF"),
        (Some(3), "가\t12.34\t5.6"),
    ];
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    for (i, (kind, text)) in rows.iter().enumerate() {
        if i > 0 {
            core.split_paragraph_native(0, i - 1, rows[i - 1].1.chars().count(), None)
                .unwrap();
        }
        core.insert_text_native(0, i, 0, text).unwrap();
        if let Some(kind) = kind {
            core.apply_para_format_native(0, i, &tabs_json(*kind))
                .unwrap();
        }
    }
    let saved = DocumentCore::from_bytes(&core.export_hwp_with_adapter().unwrap()).unwrap();
    let native = DocumentCore::from_bytes(&core.export_hwp_native().unwrap()).unwrap();
    for reopened in [&saved, &native] {
        for (i, (kind, text)) in rows.iter().enumerate() {
            check_paragraph(
                &format!("문단 {i}"),
                text,
                &stored_tabs(&reopened.document().sections[0].paragraphs[i]),
                u16::from(kind.unwrap_or(0)) + 1,
                &|offset| x_of(core.get_cursor_rect_native(0, i, offset).unwrap()),
                &|offset| x_of(reopened.get_cursor_rect_native(0, i, offset).unwrap()),
            );
        }
    }
    // 원본 문서는 바뀌지 않는다(저장 스냅숏에만 적는다).
    assert!(core.document().sections[0].paragraphs[2]
        .tab_extended
        .iter()
        .all(|ext| ext[0] == 0 && ext[1] == 0));
}

#[test]
fn hwp_save_writes_tab_widths_inside_table_cells() {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    let created: serde_json::Value =
        serde_json::from_str(&core.create_table_native(0, 0, 0, 1, 1).unwrap()).unwrap();
    let para = created["paraIdx"].as_u64().unwrap() as usize;
    let control = created["controlIdx"].as_u64().unwrap() as usize;
    let text = "가\tABC";
    core.insert_text_in_cell_native(0, para, control, 0, 0, 0, text)
        .unwrap();
    core.apply_para_format_in_cell_native(0, para, control, 0, 0, &tabs_json(1))
        .unwrap();
    let reopened = DocumentCore::from_bytes(&core.export_hwp_with_adapter().unwrap()).unwrap();
    let rhwp::model::control::Control::Table(table) =
        &reopened.document().sections[0].paragraphs[para].controls[control]
    else {
        panic!("표");
    };
    check_paragraph(
        "셀",
        text,
        &stored_tabs(&table.cells[0].paragraphs[0]),
        2,
        &|offset| {
            x_of(
                core.get_cursor_rect_in_cell_native(0, para, control, 0, 0, offset)
                    .unwrap(),
            )
        },
        &|offset| {
            x_of(
                reopened
                    .get_cursor_rect_in_cell_native(0, para, control, 0, 0, offset)
                    .unwrap(),
            )
        },
    );
}

#[test]
fn hwp_save_writes_tab_widths_in_headers() {
    // 같은 글·탭 정지의 본문 문단과 머리말 문단은 같은 폭을 적는다(머리말 너비 = 본문 너비).
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    let text = "가\tABC";
    core.insert_text_native(0, 0, 0, text).unwrap();
    core.apply_para_format_native(0, 0, &tabs_json(1)).unwrap();
    core.create_header_footer_native(0, true, 0).unwrap();
    core.insert_text_in_header_footer_native(0, true, 0, 0, 0, text)
        .unwrap();
    core.apply_para_format_in_hf_native(0, true, 0, 0, &tabs_json(1))
        .unwrap();
    let reopened = DocumentCore::from_bytes(&core.export_hwp_with_adapter().unwrap()).unwrap();
    let section = &reopened.document().sections[0];
    let header = section
        .paragraphs
        .iter()
        .flat_map(|para| &para.controls)
        .find_map(|control| match control {
            rhwp::model::control::Control::Header(header) => Some(header),
            _ => None,
        })
        .expect("머리말");
    let body = stored_tabs(&section.paragraphs[0]);
    let head = stored_tabs(&header.paragraphs[0]);
    assert_eq!(body.len(), 1);
    assert!(body[0].0 > 0 && body[0].1 == 2, "본문 {body:?}");
    assert_eq!(head.len(), 1, "머리말 탭 확장 자료");
    assert!(
        head[0].0.abs_diff(body[0].0) <= 1 && head[0].1 == 2,
        "머리말 {head:?} != 본문 {body:?}"
    );
}
