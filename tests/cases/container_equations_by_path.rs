//! 관계형. 표 칸·글상자·그림 캡션·머리말 안 수식을 쪽 개체 목록이 편집 주소(`parentParaIdx`·`cellPath`·
//! `innerControlIdx`, 머리말은 `headerFooter`)로 알리고, 그 주소로 수식 속성을 읽고 바꾸며 지운다.
//! 지운 결과는 HWP·HWPX 저장 뒤에도 남는다. 한/글 12.30은 표 칸·글상자·캡션·머리말 안 수식도
//! 눌러 고르고 두 번 눌러 고치며 지운다.

use rhwp::wasm_api::HwpDocument;
use serde_json::Value;

type Path = Vec<(usize, usize, usize)>;

fn png() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
        0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
}

fn json_path(path: &[(usize, usize, usize)]) -> String {
    serde_json::json!(path
        .iter()
        .map(|&(c, cell, p)| serde_json::json!({
            "controlIndex": c, "cellIndex": cell, "cellParaIndex": p
        }))
        .collect::<Vec<_>>())
    .to_string()
}

fn parse(json: &str) -> Value {
    serde_json::from_str(json).unwrap()
}

/// 쪽 개체 목록의 수식 편집 주소: (뿌리 문단, 경로, 수식 번호, 스크립트를 읽은 결과).
fn equation_addresses(doc: &HwpDocument) -> Vec<(usize, Path, usize)> {
    let list = parse(&doc.get_page_control_layout(0).unwrap());
    list["controls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == "equation")
        .map(|c| {
            let n = |v: &Value| v.as_u64().unwrap() as usize;
            if let Some(hf) = c.get("headerFooter") {
                // 머리말 바로 안 수식은 머리말 컨트롤 마디로 잇는다.
                return (
                    n(&hf["outerParaIdx"]),
                    vec![(n(&hf["outerControlIdx"]), 0, n(&c["paraIdx"]))],
                    n(&c["controlIdx"]),
                );
            }
            let path = c["cellPath"]
                .as_array()
                .unwrap_or_else(|| panic!("수식에 cellPath 가 없다: {c}"))
                .iter()
                .map(|e| {
                    (
                        n(&e["controlIndex"]),
                        n(&e["cellIndex"]),
                        n(&e["cellParaIndex"]),
                    )
                })
                .collect();
            (n(&c["parentParaIdx"]), path, n(&c["innerControlIdx"]))
        })
        .collect()
}

fn script(doc: &HwpDocument, (para, path, ci): &(usize, Path, usize)) -> String {
    let props = parse(
        &doc.get_equation_properties_by_path(0, *para as u32, &json_path(path), *ci as u32)
            .unwrap(),
    );
    props["script"].as_str().unwrap().to_string()
}

/// 머리말·표 칸·글상자·그림 캡션에 수식을 하나씩 넣은 문서. 칸과 글상자 수식은 글 뒤에 둔다
/// (글 없이 수식만 든 칸 문단은 표 배치가 따로 그린다).
fn document() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, "본문").unwrap();
    let hf = parse(&doc.create_header_footer(0, true, 0).unwrap());
    let hf_ctrl = hf["controlIndex"].as_u64().unwrap() as usize;
    doc.insert_equation_by_path(0, 0, &json_path(&[(hf_ctrl, 0, 0)]), 0, "h", 1000, 0)
        .unwrap();
    let table = parse(&doc.create_table(0, 0, 2, 1, 2).unwrap());
    let (tp, tc) = (
        table["paraIdx"].as_u64().unwrap() as u32,
        table["controlIdx"].as_u64().unwrap() as usize,
    );
    // 수식만 든 칸(A1)과 글 뒤 수식이 든 칸(B1).
    doc.insert_equation_by_path(0, tp, &json_path(&[(tc, 0, 0)]), 0, "a", 1000, 0)
        .unwrap();
    doc.insert_text_in_cell_by_path_api(0, tp, &json_path(&[(tc, 1, 0)]), 0, "칸")
        .unwrap();
    doc.insert_equation_by_path(0, tp, &json_path(&[(tc, 1, 0)]), 1, "b", 1000, 0)
        .unwrap();
    let shape = parse(
        &doc.create_shape_control(
            r#"{"sectionIdx":0,"paraIdx":0,"charOffset":0,"width":20000,"height":15000,"horzOffset":3000,"vertOffset":30000,"treatAsChar":false,"textWrap":"InFrontOfText","shapeType":"textbox"}"#,
        )
        .unwrap(),
    );
    let box_ctrl = shape["controlIdx"].as_u64().unwrap() as usize;
    doc.insert_equation_by_path(0, 0, &json_path(&[(box_ctrl, 0, 0)]), 0, "t", 1000, 0)
        .unwrap();
    let pic = parse(
        &doc.insert_picture(0, 0, 0, "", &png(), 9600, 9600, 1, 1, "png", "", None, None)
            .unwrap(),
    );
    let pic_ctrl = pic["controlIdx"].as_u64().unwrap() as u32;
    doc.set_picture_properties(
        0,
        0,
        pic_ctrl,
        r#"{"treatAsChar":false,"horzOffset":30000,"vertOffset":50000,"textWrap":"InFrontOfText","hasCaption":true,"captionDirection":"Bottom"}"#,
    )
    .unwrap();
    doc.insert_equation_by_path(
        0,
        0,
        &json_path(&[(pic_ctrl as usize, 0, 0)]),
        0,
        "c",
        1000,
        0,
    )
    .unwrap();
    doc
}

#[test]
fn container_equations_report_edit_address_and_delete_by_path() {
    let mut doc = document();
    let found = equation_addresses(&doc);
    let mut scripts: Vec<String> = found.iter().map(|a| script(&doc, a)).collect();
    scripts.sort();
    assert_eq!(scripts, ["a", "b", "c", "h", "t"], "{found:?}");

    // 주소로 고친 수식은 다시 배치해도 같은 주소로 읽힌다.
    let b = found
        .iter()
        .find(|a| script(&doc, a) == "b")
        .unwrap()
        .clone();
    doc.set_equation_properties_by_path(
        0,
        b.0 as u32,
        &json_path(&b.1),
        b.2 as u32,
        r#"{"script":"b over 2"}"#,
    )
    .unwrap();
    assert!(equation_addresses(&doc)
        .iter()
        .any(|a| script(&doc, a) == "b over 2"));

    // 수식이 아닌 컨트롤 번호는 지우지 않는다.
    assert!(doc
        .delete_equation_control_by_path_native(0, b.0, &b.1, b.2 + 1)
        .is_err());
    for target in ["a", "b over 2", "c", "h", "t"] {
        let address = equation_addresses(&doc)
            .into_iter()
            .find(|a| script(&doc, a) == target)
            .unwrap();
        let ok = doc
            .delete_equation_control_by_path(
                0,
                address.0 as u32,
                &json_path(&address.1),
                address.2 as u32,
            )
            .unwrap();
        assert_eq!(ok, r#"{"ok":true}"#);
        assert!(!equation_addresses(&doc)
            .iter()
            .any(|a| script(&doc, a) == target));
    }
    assert!(equation_addresses(&doc).is_empty());
    for bytes in [
        doc.export_hwp_native().unwrap(),
        doc.export_hwpx_native().unwrap(),
    ] {
        let reopened = HwpDocument::from_bytes(&bytes).unwrap();
        assert!(equation_addresses(&reopened).is_empty());
        let text = parse(&reopened.get_page_control_layout(0).unwrap());
        assert!(text["controls"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["type"] == "table"));
    }
}
