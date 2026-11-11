//! 관계형. 저장 폭이 있는 오른쪽 탭(채움 없음)이 런의 마지막 탭이어도 그 탭 정지에 맞춘다. 한컴 12.30이
//! 40mm 오른쪽 탭으로 저장한 문단은 탭 뒤 낱말의 끝이 40mm에 서는데, 저장본을 다시 열면 본문
//! 오른쪽 끝으로 끌렸다. 기준: 저장 폭을 실은 저장본의 캐럿 = 자리표로 조판한 캐럿.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::DocumentCore;

fn caret_x(core: &DocumentCore, offset: usize) -> f64 {
    let rect: serde_json::Value =
        serde_json::from_str(&core.get_cursor_rect_native(0, 0, offset).unwrap()).unwrap();
    rect["x"].as_f64().unwrap()
}

#[test]
fn stored_last_right_tab_keeps_its_stop() {
    // 한 런 안에서 탭 뒤에 글이 이어지는 형상(교차 런 탭이 아니다).
    let text = "AB\tCD";
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.insert_text_native(0, 0, 0, text).unwrap();
    // 40mm 오른쪽 탭(TabItem.position은 2배 HWPUNIT).
    let stop = (40.0 * 7200.0 * 2.0 / 25.4_f64).round() as u32;
    core.apply_para_format_native(
        0,
        0,
        &format!(r#"{{"tabStops":[{{"position":{stop},"type":1,"fill":0}}]}}"#),
    )
    .unwrap();
    let laid: Vec<f64> = (0..=text.chars().count())
        .map(|offset| caret_x(&core, offset))
        .collect();
    // 한컴처럼 탭 전진(HWPUNIT)과 오른쪽 탭 종류(`(1 + 1) << 8`)를 싣는다.
    let tab = text.chars().position(|ch| ch == '\t').unwrap();
    let width = ((laid[tab + 1] - laid[tab]) * 75.0).round() as u16;
    {
        let doc = core.document_mut();
        doc.sections[0].paragraphs[0].tab_extended = vec![[width, 0, 0x0200, 0, 0, 0, 9]];
        doc.sections[0].raw_stream = None;
    }
    let reopened = DocumentCore::from_bytes(&core.export_hwp_native().unwrap()).unwrap();
    assert_eq!(
        reopened.document().sections[0].paragraphs[0].tab_extended[0][0],
        width,
        "저장 폭이 실린다"
    );
    for (offset, expected) in laid.iter().enumerate() {
        let actual = caret_x(&reopened, offset);
        assert!(
            (actual - expected).abs() < 0.6,
            "캐럿 {offset}: {actual} != {expected}"
        );
    }
}
