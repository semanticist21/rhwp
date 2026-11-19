//! 머리말 영역보다 큰 머리말 표. 머리말 영역 아래 끝을 넘어 그린 표 칸·캡션을 누르면 머리말 히트이고,
//! 본문 히트는 머리말 표의 칸·캡션 주소를 본문 주소처럼 주지 않는다. 전에는 머리말 히트가 머리말 영역
//! 사각형만 봐 넘친 자리를 본문으로 보냈고, 본문 히트는 머리말 표 캡션을 `(머리말 문단, 표 컨트롤, 65534)`
//! 본문 주소로 줘 그 주소를 본문에서 찾다 "셀 문단 접근 실패"가 났다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::wasm_api::HwpDocument;
use serde_json::Value;

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

/// 머리말에 기본 4×4 표를 넣고 아래 캡션 글 끝에 "AB"를 넣은 문서.
fn doc_with_tall_header_table() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    doc.create_header_footer_native(0, true, 0).unwrap();
    let info = json(&doc.get_header_footer_native(0, true, 0).unwrap());
    let host = info["paraIndex"].as_u64().unwrap() as usize;
    let hf = info["controlIndex"].as_u64().unwrap() as usize;
    let created = json(
        &doc.create_table_in_cell_by_path_native(
            0,
            host,
            &[(hf, 0, 0)],
            0,
            4,
            4,
            None,
            None,
            false,
        )
        .unwrap(),
    );
    let cell: Vec<(usize, usize, usize)> = created["cellPath"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            let at = |k: &str| e[k].as_u64().unwrap() as usize;
            (at("controlIndex"), at("cellIndex"), at("cellParaIndex"))
        })
        .collect();
    doc.set_table_properties_by_path_native(
        0,
        host,
        &cell,
        r#"{"hasCaption":true,"captionDirection":3}"#,
    )
    .unwrap();
    let mut caption = cell.clone();
    let last = caption.last_mut().unwrap();
    *last = (last.0, 65534, 0);
    let len = doc
        .get_text_in_cell_by_path(0, host, &caption, 0, usize::MAX / 2)
        .unwrap()
        .chars()
        .count();
    doc.insert_text_in_cell_by_path(0, host, &caption, len, "AB")
        .unwrap();
    doc
}

/// 렌더 트리에서 글에 `text`가 든 첫 글자 런의 가운데.
fn run_center(doc: &HwpDocument, text: &str) -> (f64, f64) {
    fn find(node: &Value, text: &str) -> Option<(f64, f64)> {
        if node["type"] == "TextRun" && node["text"].as_str().is_some_and(|t| t.contains(text)) {
            let b = |k: &str| node["bbox"][k].as_f64().unwrap();
            return Some((b("x") + b("w") / 2.0, b("y") + b("h") / 2.0));
        }
        node["children"]
            .as_array()?
            .iter()
            .find_map(|n| find(n, text))
    }
    let tree = json(&doc.get_page_render_tree(0).unwrap());
    find(&tree, text).expect("글자 런")
}

#[test]
fn overflowing_header_table_caption_is_a_header_hit_not_a_body_cell() {
    let doc = doc_with_tall_header_table();
    let (x, y) = run_center(&doc, "AB");
    let area = &json(&doc.get_page_info_native(0).unwrap())["headerArea"];
    let bottom = area["y"].as_f64().unwrap() + area["height"].as_f64().unwrap();
    assert!(
        y > bottom,
        "캡션 {y} 이 머리말 영역 아래 끝 {bottom} 을 넘는다"
    );

    // 넘친 캡션도 머리말이다.
    let hf = json(&doc.hit_test_header_footer_native(0, x, y).unwrap());
    assert_eq!(hf["hit"], true, "{hf}");
    assert_eq!(hf["isHeader"], true, "{hf}");

    // 본문 히트는 머리말 표 캡션을 본문 표 주소로 주지 않는다.
    let body = json(&doc.hit_test_native(0, x, y).unwrap());
    assert!(
        body.get("parentParaIndex").is_none() && body.get("cellPath").is_none(),
        "본문 히트가 머리말 표 칸을 줬다: {body}"
    );

    // 본문 한가운데는 여전히 머리말이 아니다.
    let info = json(&doc.get_page_info_native(0).unwrap());
    let mid = info["height"].as_f64().unwrap() / 2.0;
    let miss = json(&doc.hit_test_header_footer_native(0, x, mid).unwrap());
    assert_eq!(miss["hit"], false, "{miss}");
}
