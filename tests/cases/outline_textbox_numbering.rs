//! 관계형. 글상자 안 개요 문단은 한/글 12.30처럼 문서 순서로 본문과 한 계수기로 센다.
//! 시험 문서는 한/글 12.30 Mac이 저장한 HWPX다(본문 개요 1 · 표 셀 개요 2 · 글상자 개요 1 ·
//! 본문 개요 1). 한/글 화면과 개요 보기: `1.` · `가.` · 글상자 `2.` · `3.`.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::DocumentCore;

fn fixture() -> DocumentCore {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/outline_textbox/textbox-outline.hwpx"
    ))
    .unwrap();
    DocumentCore::from_bytes(&bytes).unwrap()
}

/// SVG 글자 노드의 글을 공백 없이 잇는다.
fn svg_text(svg: &str) -> String {
    svg.split("<text")
        .skip(1)
        .filter_map(|chunk| {
            let body = &chunk[chunk.find('>')? + 1..];
            Some(body[..body.find("</text>")?].to_owned())
        })
        .collect::<String>()
        .split_whitespace()
        .collect()
}

#[test]
fn text_box_outline_is_listed_in_document_order() {
    let core = fixture();
    let nav: serde_json::Value =
        serde_json::from_str(&core.get_outline_navigation_native().unwrap()).unwrap();
    let rows: Vec<(String, String, bool)> = nav["outline"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            (
                item["number"].as_str().unwrap().to_owned(),
                item["title"].as_str().unwrap().to_owned(),
                item.get("cellPath").is_some(),
            )
        })
        .collect();
    let expected = [
        ("1.", "인라인 개체 이동 확인", false),
        ("가.", "표 내용", true),
        ("2.", "글상자 내용", true),
        ("3.", "새 자리: 가나다", false),
    ];
    assert_eq!(
        rows,
        expected.map(|(n, t, c)| (n.to_owned(), t.to_owned(), c))
    );
}

#[test]
fn text_box_outline_paragraph_draws_the_shared_counter() {
    let text = svg_text(&fixture().render_page_svg_native(0).unwrap());
    assert!(text.contains("2.글상자내용"), "{text}");
    assert!(text.contains("3.새자리:가나다"), "{text}");
}
