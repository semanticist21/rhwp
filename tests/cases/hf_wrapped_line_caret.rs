//! 머리말·꼬리말의 줄 경계 캐럿은 다음 줄에 속하고 문단 끝·빈 문단은 유지한다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::wasm_api::HwpDocument;
use serde_json::Value;

fn fixture(header: bool, text: &str) -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.create_header_footer_native(0, header, 0).unwrap();
    doc.insert_text_in_header_footer_native(0, header, 0, 0, 0, text)
        .unwrap();
    doc
}

fn caret(doc: &HwpDocument, header: bool, paragraph: usize, offset: usize) -> Value {
    serde_json::from_str(
        &doc.get_cursor_rect_in_header_footer_native(0, header, 0, paragraph, offset, 0)
            .unwrap(),
    )
    .unwrap()
}

fn rects(doc: &HwpDocument, header: bool, start: usize, end: usize) -> Vec<Value> {
    serde_json::from_str(
        &doc.get_selection_rects_in_header_footer_native(0, header, 0, 0, 0, start, 0, end)
            .unwrap(),
    )
    .unwrap()
}

fn hit(doc: &HwpDocument, header: bool, x: f64, y: f64) -> Value {
    serde_json::from_str(
        &doc.hit_test_in_header_footer_target_native(0, 0, header, 0, x, y)
            .unwrap(),
    )
    .unwrap()
}

fn number(value: &Value, key: &str) -> f64 {
    value[key].as_f64().unwrap()
}

fn assert_wrapped_boundary(doc: &HwpDocument, header: bool) {
    let rows = rects(doc, header, 0, 70);
    assert_eq!(rows.len(), 2, "70개 글자는 실제 두 줄이어야 한다: {rows:?}");
    let second = &rows[1];
    let at_start = hit(
        doc,
        header,
        number(second, "x"),
        number(second, "y") + number(second, "height") / 2.0,
    );
    assert_eq!(at_start["hit"], true);
    let boundary = at_start["charOffset"].as_u64().unwrap() as usize;
    assert!(boundary > 0 && boundary < 70);
    // 같은 글자 앞의 클릭과 캐럿 조회는 같은 줄 시작을 가리켜야 한다.
    let current = caret(doc, header, 0, boundary);
    assert_eq!(current, at_start["cursorRect"]);
    assert!(number(&current, "y") > number(&caret(doc, header, 0, boundary - 1), "y"));
    assert_eq!(
        number(&current, "y"),
        number(&caret(doc, header, 0, boundary + 1), "y")
    );
    let at_end = hit(
        doc,
        header,
        number(second, "x") + number(second, "width") + 1.0,
        number(second, "y") + number(second, "height") / 2.0,
    );
    assert_eq!(at_end["charOffset"], 70);
    assert_eq!(caret(doc, header, 0, 70), at_end["cursorRect"]);
}

#[test]
fn wrapped_line_start_matches_hit_before_and_after_both_exports() {
    for header in [true, false] {
        let doc = fixture(header, &"앞".repeat(70));
        assert_wrapped_boundary(&doc, header);
        for hwpx in [false, true] {
            let bytes = if hwpx {
                doc.export_hwpx().unwrap()
            } else {
                doc.export_hwp().unwrap()
            };
            let reopened = HwpDocument::from_bytes(&bytes).unwrap();
            assert_wrapped_boundary(&reopened, header);
        }
    }
}

#[test]
fn paragraph_end_keeps_the_last_run_end() {
    for header in [true, false] {
        let doc = fixture(header, "앞뒤");
        let start = caret(&doc, header, 0, 0);
        let end = caret(&doc, header, 0, 2);
        assert_eq!(number(&start, "y"), number(&end, "y"));
        assert!(number(&end, "x") > number(&start, "x"));
        let row = &rects(&doc, header, 0, 2)[0];
        let at_end = hit(
            &doc,
            header,
            number(row, "x") + number(row, "width") + 1.0,
            number(row, "y") + number(row, "height") / 2.0,
        );
        assert_eq!(at_end["charOffset"], 2);
        assert_eq!(end, at_end["cursorRect"]);
    }
}

#[test]
fn empty_paragraph_keeps_its_own_line_between_text_paragraphs() {
    for header in [true, false] {
        let mut doc = fixture(header, "앞");
        doc.split_paragraph_in_header_footer_native(0, header, 0, 0, 1, None)
            .unwrap();
        doc.split_paragraph_in_header_footer_native(0, header, 0, 1, 0, None)
            .unwrap();
        doc.insert_text_in_header_footer_native(0, header, 0, 2, 0, "뒤")
            .unwrap();
        let first = caret(&doc, header, 0, 0);
        let empty = caret(&doc, header, 1, 0);
        let last = caret(&doc, header, 2, 0);
        assert!(number(&first, "y") < number(&empty, "y"));
        assert!(number(&empty, "y") < number(&last, "y"));
        assert_eq!(number(&empty, "x"), number(&first, "x"));
        assert!(number(&empty, "height") > 0.0);
        let at_empty = hit(
            &doc,
            header,
            number(&empty, "x"),
            number(&empty, "y") + number(&empty, "height") / 2.0,
        );
        assert_eq!(at_empty["paraIndex"], 1);
        assert_eq!(at_empty["charOffset"], 0);
        assert_eq!(empty, at_empty["cursorRect"]);
    }
}

#[test]
fn style_run_boundary_uses_the_following_character_metrics() {
    for header in [true, false] {
        let mut doc = fixture(header, "앞중뒤");
        for (start, end, size) in [(0, 1, 2400), (1, 3, 1200)] {
            doc.apply_char_format_in_header_footer_native(
                0,
                header,
                0,
                0,
                start,
                0,
                end,
                &format!(r#"{{"fontSize":{size}}}"#),
            )
            .unwrap();
        }
        let first = caret(&doc, header, 0, 0);
        let boundary = caret(&doc, header, 0, 1);
        let following = caret(&doc, header, 0, 2);
        assert_eq!(number(&first, "height"), 32.0);
        assert_eq!(number(&boundary, "height"), 16.0);
        assert_eq!(number(&boundary, "y"), number(&following, "y"));
        let character = &rects(&doc, header, 1, 2)[0];
        // 선택 사각형은 소수 둘째 자리, 캐럿은 첫째 자리로 공개된다.
        assert!((number(&boundary, "x") - number(character, "x")).abs() <= 0.06);
        assert_eq!(number(&boundary, "height"), number(&following, "height"));
    }
}
