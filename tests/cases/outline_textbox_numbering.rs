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

/// 개요 경로가 가리키는 글상자 문단에 캐럿·선택이 서고, 그 캐럿 자리를 누르면 같은 경로가 온다.
/// 글상자 안 글상자 문서는 한/글 12.30 Mac이 다시 저장한 HWP이고, 표 셀 안 묶음 문서는 rhwp로 만든 HWP다.
/// 둘 다 한/글 12.30 개요 보기가 같은 개요를 보이고, 항목을 고르면 글상자 안의 그 문단을 블록으로 고른다.
fn assert_text_box_outline_targets(file: &str, titles: &[&str]) {
    let path = format!(
        "{}/tests/fixtures/outline_textbox/{file}",
        env!("CARGO_MANIFEST_DIR")
    );
    let doc = rhwp::wasm_api::HwpDocument::from_bytes(&std::fs::read(path).unwrap()).unwrap();
    let nav: serde_json::Value =
        serde_json::from_str(&doc.get_outline_navigation_native().unwrap()).unwrap();
    for title in titles {
        let item = nav["outline"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["title"] == *title)
            .unwrap_or_else(|| panic!("{file}: {title}"));
        let cell_path = &item["cellPath"];
        let steps = cell_path.as_array().unwrap();
        assert_eq!(steps.len(), 2, "{file}: {title}");
        let (sec, para) = (
            item["section"].as_u64().unwrap() as u32,
            item["paragraph"].as_u64().unwrap() as u32,
        );
        let path_json = cell_path.to_string();
        let caret: serde_json::Value = serde_json::from_str(
            &doc.get_cursor_rect_by_path(sec, para, &path_json, 1)
                .unwrap_or_else(|_| panic!("{file}: {title} 캐럿")),
        )
        .unwrap();
        let hit: serde_json::Value = serde_json::from_str(
            &doc.hit_test_native(
                caret["pageIndex"].as_u64().unwrap() as u32,
                caret["x"].as_f64().unwrap() + 1.0,
                caret["y"].as_f64().unwrap() + caret["height"].as_f64().unwrap() / 2.0,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(hit["cellPath"], *cell_path, "{file}: {title} 누르기");
        let cell_para = steps[1]["cellParaIndex"].as_u64().unwrap() as u32;
        let length = title.chars().count() as u32;
        let rects: serde_json::Value = serde_json::from_str(
            &doc.get_selection_rects_in_cell_by_path(
                sec, para, &path_json, cell_para, 0, cell_para, length,
            )
            .unwrap(),
        )
        .unwrap();
        assert!(
            !rects.as_array().unwrap().is_empty(),
            "{file}: {title} 선택"
        );
    }
}

#[test]
fn nested_text_box_outline_takes_the_caret() {
    assert_text_box_outline_targets("nested-textbox.hwp", &["안쪽 글상자"]);
}

#[test]
fn grouped_text_box_in_cell_outline_takes_the_caret() {
    assert_text_box_outline_targets("cell-group-textbox.hwp", &["첫 상자", "둘째 상자"]);
}
