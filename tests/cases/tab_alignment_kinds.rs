//! 관계형. 사용자 탭 정지의 오른쪽·가운데·소수점 정렬을 캐럿 좌표로 검사한다. 기준은 한컴 12.30 Mac에서
//! 같은 글(`가⇥ABC⇥DEF`, `가⇥12.34⇥5.6`, `가⇥1234⇥x.5`)에 40mm·80mm 탭을 둔 화면이다. 오른쪽 탭은
//! 낱말 끝, 가운데 탭은 낱말 가운데, 소수점 탭은 첫 소수점(없으면 끝)을 탭 위치에 맞추며, 줄의 마지막
//! 탭이 아니어도 같다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::DocumentCore;

/// TabItem.position은 문단 여백처럼 2배 HWPUNIT이다.
fn tab_pos(mm: f64) -> u32 {
    (mm * 7200.0 * 2.0 / 25.4).round() as u32
}

fn px(mm: f64) -> f64 {
    mm * 96.0 / 25.4
}

/// 문단마다 (탭 종류, 글). 탭은 40mm·80mm 두 개다.
fn document(rows: &[(u8, &str)]) -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    for (i, (kind, text)) in rows.iter().enumerate() {
        if i > 0 {
            core.split_paragraph_native(0, i - 1, rows[i - 1].1.chars().count(), None)
                .unwrap();
        }
        core.insert_text_native(0, i, 0, text).unwrap();
        let tabs = format!(
            r#"{{"tabStops":[{{"position":{},"type":{kind},"fill":0}},{{"position":{},"type":{kind},"fill":0}}]}}"#,
            tab_pos(40.0),
            tab_pos(80.0)
        );
        core.apply_para_format_native(0, i, &tabs).unwrap();
    }
    core
}

fn caret_x(core: &DocumentCore, para: usize, offset: usize) -> f64 {
    let rect: serde_json::Value =
        serde_json::from_str(&core.get_cursor_rect_native(0, para, offset).unwrap()).unwrap();
    rect["x"].as_f64().unwrap()
}

#[test]
fn right_center_and_decimal_tabs_align_every_segment() {
    let core = document(&[
        (1, "가\tABC\tDEF"),
        (2, "가\tABC\tDEF"),
        (3, "가\t12.34\t5.6"),
        (3, "가\t1234\tx.5"),
    ]);
    // 본문 왼쪽(왼쪽 여백 30mm) 기준 탭 위치.
    let left = caret_x(&core, 0, 0);
    let (first, second) = (left + px(40.0), left + px(80.0));
    let near = |actual: f64, expected: f64, what: &str| {
        assert!(
            (actual - expected).abs() < 0.6,
            "{what}: {actual} != {expected}"
        );
    };
    // 오른쪽: 낱말 끝.
    near(caret_x(&core, 0, 5), first, "오른쪽 탭 ABC 끝");
    near(caret_x(&core, 0, 9), second, "오른쪽 탭 DEF 끝");
    // 가운데: 낱말 가운데.
    near(
        (caret_x(&core, 1, 2) + caret_x(&core, 1, 5)) / 2.0,
        first,
        "가운데 탭 ABC 가운데",
    );
    near(
        (caret_x(&core, 1, 6) + caret_x(&core, 1, 9)) / 2.0,
        second,
        "가운데 탭 DEF 가운데",
    );
    // 소수점: 첫 소수점 앞, 소수점이 없으면 끝.
    near(caret_x(&core, 2, 4), first, "소수점 탭 12.34의 소수점");
    near(caret_x(&core, 2, 9), second, "소수점 탭 5.6의 소수점");
    near(caret_x(&core, 3, 6), first, "소수점 탭 1234의 끝");
    near(caret_x(&core, 3, 8), second, "소수점 탭 x.5의 소수점");
}
