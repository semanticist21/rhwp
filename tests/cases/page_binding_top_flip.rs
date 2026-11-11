//! 관계형. 편집 용지의 위로 넘기기 제본(`binding: 2`)을 캐럿 좌표로 검사한다. 기준은 한컴 12.30 Mac에서
//! 제본 여백 20mm·위로 넘기기 세 쪽 문서(머리말·꼬리말 포함)를 연 화면이다. 제본 여백은 홀수 쪽 위,
//! 짝수 쪽 아래에 붙고 왼쪽에는 붙지 않으며, 머리말·꼬리말도 함께 옮겨진다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::DocumentCore;

fn mm(v: f64) -> u32 {
    (v * 7200.0 / 25.4).round() as u32
}

fn px(v: f64) -> f64 {
    v * 96.0 / 25.4
}

fn caret(core: &DocumentCore, para: usize) -> (u64, f64, f64) {
    let rect: serde_json::Value =
        serde_json::from_str(&core.get_cursor_rect_native(0, para, 0).unwrap()).unwrap();
    (
        rect["pageIndex"].as_u64().unwrap(),
        rect["x"].as_f64().unwrap(),
        rect["y"].as_f64().unwrap(),
    )
}

/// 쪽 정보의 본문 위·아래 경계.
fn body(core: &DocumentCore, page: u32) -> (f64, f64) {
    let info: serde_json::Value =
        serde_json::from_str(&core.get_page_info_native(page).unwrap()).unwrap();
    (
        info["bodyTop"].as_f64().unwrap(),
        info["bodyBottom"].as_f64().unwrap(),
    )
}

#[test]
fn top_flip_puts_the_gutter_above_odd_pages_and_below_even_pages() {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    let texts = ["첫 쪽", "둘째 쪽", "셋째 쪽"];
    for (para, text) in texts.iter().enumerate() {
        if para > 0 {
            core.insert_page_break_native(0, para - 1, texts[para - 1].chars().count())
                .unwrap();
        }
        core.insert_text_native(0, para, 0, text).unwrap();
    }
    let single = [caret(&core, 0), caret(&core, 1)];
    let plain = body(&core, 0);
    core.set_page_def_native(
        0,
        &format!(r#"{{"marginGutter":{},"binding":2}}"#, mm(20.0)),
    )
    .unwrap();
    let pages: Vec<_> = (0..3).map(|para| caret(&core, para)).collect();
    assert_eq!(
        pages.iter().map(|c| c.0).collect::<Vec<_>>(),
        [0, 1, 2],
        "본문 높이가 같아 쪽 나눔이 그대로다"
    );
    for (page, (_, x, y)) in pages.iter().enumerate() {
        assert!(
            (x - single[0].1).abs() < 0.1,
            "{page}쪽: 왼쪽에 제본 여백이 없다"
        );
        let expected = if page % 2 == 0 {
            single[0].2 + px(20.0)
        } else {
            single[1].2
        };
        assert!((y - expected).abs() < 0.1, "{page}쪽 y {y} != {expected}");
    }
    // 쪽 정보의 본문 경계도 제본 여백을 반영한다(본문 모서리 표시·격자가 쓴다).
    for page in 0..3u32 {
        let (top, bottom) = body(&core, page);
        let (expected_top, expected_bottom) = if page % 2 == 0 {
            (plain.0 + px(20.0), plain.1)
        } else {
            (plain.0, plain.1 - px(20.0))
        };
        assert!((top - expected_top).abs() < 0.1, "{page}쪽 본문 위 {top}");
        assert!(
            (bottom - expected_bottom).abs() < 0.1,
            "{page}쪽 본문 아래 {bottom}"
        );
        assert!(
            pages[page as usize].2 >= top,
            "{page}쪽 첫 줄은 본문 위 경계 안"
        );
    }
    for bytes in [
        core.export_hwp_native().unwrap(),
        core.export_hwpx_native().unwrap(),
    ] {
        let reopened = DocumentCore::from_bytes(&bytes).unwrap();
        assert!((caret(&reopened, 0).2 - pages[0].2).abs() < 0.1);
        assert!((caret(&reopened, 1).2 - pages[1].2).abs() < 0.1);
    }
}
