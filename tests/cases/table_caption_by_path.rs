//! 칸 안 표·머리말 표의 캡션을 셀 경로로 고친다. 경로 마디의 셀 65534는 그 표의 캡션이다
//! (본문 표 셀 번호 API·가변 경로 해석기와 같은 꼴). 전에는 경로 조회가 표의 칸만 찾아
//! `insertTextInCellByPath`·`getCellParagraphLengthByPath`가 "셀 65534 범위 초과"로 실패했다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::control::Control;
use rhwp::model::table::Table;
use rhwp::wasm_api::HwpDocument;

const CAPTION: usize = 65534;
type Path = Vec<(usize, usize, usize)>;

fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap()
}

fn created_path(text: &str) -> Path {
    json(text)["cellPath"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            let at = |key: &str| entry[key].as_u64().unwrap() as usize;
            (at("controlIndex"), at("cellIndex"), at("cellParaIndex"))
        })
        .collect()
}

/// 본문 1×2 표 첫 칸 안의 2×2 표와 머리말의 2×2 표. (뿌리 문단, 안쪽 표 첫 칸 경로)다.
fn doc_with_inner_tables() -> (HwpDocument, usize, Path, usize, Path) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    let outer = json(&doc.create_table_native(0, 0, 0, 1, 2).unwrap());
    let body = outer["paraIdx"].as_u64().unwrap() as usize;
    let control = outer["controlIdx"].as_u64().unwrap_or(0) as usize;
    let nested = created_path(
        &doc.create_table_in_cell_by_path_native(
            0,
            body,
            &[(control, 0, 0)],
            0,
            2,
            2,
            None,
            None,
            false,
        )
        .unwrap(),
    );
    doc.create_header_footer_native(0, true, 0).unwrap();
    let info = json(&doc.get_header_footer_native(0, true, 0).unwrap());
    let host = info["paraIndex"].as_u64().unwrap() as usize;
    let hf = info["controlIndex"].as_u64().unwrap() as usize;
    let header = created_path(
        &doc.create_table_in_cell_by_path_native(
            0,
            host,
            &[(hf, 0, 0)],
            0,
            2,
            2,
            None,
            None,
            false,
        )
        .unwrap(),
    );
    (doc, body, nested, host, header)
}

fn caption_path(cell: &Path) -> Path {
    let mut path = cell.clone();
    let last = path.last_mut().unwrap();
    *last = (last.0, CAPTION, 0);
    path
}

fn path_json(path: &Path) -> String {
    serde_json::to_string(
        &path
            .iter()
            .map(|&(c, cell, p)| {
                serde_json::json!({"controlIndex": c, "cellIndex": cell, "cellParaIndex": p})
            })
            .collect::<Vec<_>>(),
    )
    .unwrap()
}

fn body_table(doc: &HwpDocument, para: usize) -> &Table {
    doc.document().sections[0].paragraphs[para]
        .controls
        .iter()
        .find_map(|control| match control {
            Control::Table(table) => Some(table.as_ref()),
            _ => None,
        })
        .unwrap()
}

#[test]
fn caption_of_nested_and_header_table_is_edited_by_path() {
    let (mut doc, body, nested, host, header) = doc_with_inner_tables();
    let targets = [(body, nested), (host, header)];
    for (root, cell) in &targets {
        let (root, cell) = (*root, cell);
        doc.set_table_properties_by_path_native(
            0,
            root,
            cell,
            r#"{"hasCaption":true,"captionDirection":3}"#,
        )
        .unwrap();
        let caption = caption_path(cell);
        let before = doc
            .get_cell_paragraph_length_by_path(0, root as u32, &path_json(&caption))
            .unwrap() as usize;
        doc.insert_text_in_cell_by_path(0, root, &caption, before, "AB")
            .unwrap();
        let text = doc
            .get_text_in_cell_by_path(0, root, &caption, 0, usize::MAX / 2)
            .unwrap();
        assert!(text.ends_with("AB"), "안쪽 표 캡션에 든다: {text:?}");
        assert_eq!(
            doc.get_cell_paragraph_count_by_path(0, root as u32, &path_json(&caption))
                .unwrap(),
            1
        );
        // 캡션 캐럿은 그 캡션 줄에서 찾는다.
        let rect = json(
            &doc.get_cursor_rect_by_path(0, root as u32, &path_json(&caption), before as u32 + 2)
                .unwrap(),
        );
        // 아래 캡션이므로 그 표 마지막 줄 칸의 캐럿보다 아래에 있다.
        let mut last_row = cell.clone();
        last_row.last_mut().unwrap().1 = 3;
        let row = json(
            &doc.get_cursor_rect_by_path(0, root as u32, &path_json(&last_row), 0)
                .unwrap(),
        );
        let row_bottom = row["y"].as_f64().unwrap() + row["height"].as_f64().unwrap();
        assert!(
            rect["y"].as_f64().unwrap() >= row_bottom,
            "캡션 캐럿 {rect} 은 표 마지막 줄 {row} 아래"
        );
    }
    // 바깥 본문 표에는 캡션이 생기지 않는다.
    assert!(body_table(&doc, body).caption.is_none());

    for bytes in [
        doc.export_hwp_native().unwrap(),
        doc.export_hwpx_native().unwrap(),
    ] {
        let reopened = HwpDocument::from_bytes(&bytes).unwrap();
        for (root, cell) in &targets {
            let text = reopened
                .get_text_in_cell_by_path(0, *root, &caption_path(cell), 0, usize::MAX / 2)
                .unwrap();
            assert!(text.ends_with("AB"), "저장 뒤에도 남는다: {text:?}");
        }
        assert!(body_table(&reopened, body).caption.is_none());
    }
}

#[test]
fn caption_step_without_caption_is_an_error() {
    let (doc, body, nested, _, _) = doc_with_inner_tables();
    let error = doc
        .get_text_in_cell_by_path(0, body, &caption_path(&nested), 0, 1)
        .unwrap_err();
    assert!(error.to_string().contains("캡션이 없습니다"), "{error}");
}
