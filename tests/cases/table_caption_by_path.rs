//! 칸 안 표의 캡션을 셀 경로로 읽고 고친다. 경로 마디의 셀 65534는 그 표의 캡션이다
//! (본문 표 셀 번호 API·가변 경로 해석기와 같은 꼴). 전에는 불변 경로 조회가 표의 칸만 찾아
//! `getCellParagraphCountByPath`·`getCellParagraphLengthByPath`·`insertTextInCellByPath`가
//! "셀 65534 범위 초과"로 실패했고, 칸 안 표 캡션 글은 같은 번호의 본문 표 캡션 주소를 가져
//! 경로 캐럿이 그 캡션을 찾지 못했다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::control::Control;
use rhwp::model::shape::CaptionDirection;
use rhwp::model::table::Table;
use rhwp::wasm_api::HwpDocument;

// [#5875] 바깥 표 한 칸 안에 `<표 1> 공급전압 차단` 같은 글자 캡션을 단 중첩 표가 있다.
const SAMPLE: &str = "samples/issue5875/nested_table_text_caption.hwp";
const CAPTION: usize = 65534;
type Path = Vec<(usize, usize, usize)>;

fn load() -> HwpDocument {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(SAMPLE);
    HwpDocument::from_bytes(&std::fs::read(path).unwrap()).unwrap()
}

/// 위·아래 캡션을 단 첫 중첩 표: (구역, 본문 문단, 그 표 첫 칸 경로, 캡션 방향).
fn nested_captioned_table(doc: &HwpDocument) -> (usize, usize, Path, CaptionDirection) {
    for (sec, section) in doc.document().sections.iter().enumerate() {
        for (para, paragraph) in section.paragraphs.iter().enumerate() {
            for (ci, control) in paragraph.controls.iter().enumerate() {
                let Control::Table(outer) = control else {
                    continue;
                };
                for (cell, outer_cell) in outer.cells.iter().enumerate() {
                    for (cp, cell_para) in outer_cell.paragraphs.iter().enumerate() {
                        for (ni, nested) in cell_para.controls.iter().enumerate() {
                            let Control::Table(nested) = nested else {
                                continue;
                            };
                            let Some(caption) = nested.caption.as_ref() else {
                                continue;
                            };
                            if matches!(
                                caption.direction,
                                CaptionDirection::Top | CaptionDirection::Bottom
                            ) {
                                return (
                                    sec,
                                    para,
                                    vec![(ci, cell, cp), (ni, 0, 0)],
                                    caption.direction,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    panic!("{SAMPLE}: 캡션을 단 중첩 표가 있어야 한다");
}

fn with_cell(path: &Path, cell: usize) -> Path {
    let mut path = path.clone();
    let last = path.last_mut().unwrap();
    *last = (last.0, cell, 0);
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

fn nested_table<'a>(doc: &'a HwpDocument, sec: usize, para: usize, path: &Path) -> &'a Table {
    let (ci, cell, cp) = path[0];
    let Control::Table(outer) = &doc.document().sections[sec].paragraphs[para].controls[ci] else {
        unreachable!()
    };
    let Control::Table(nested) = &outer.cells[cell].paragraphs[cp].controls[path[1].0] else {
        unreachable!()
    };
    nested
}

fn caret(doc: &HwpDocument, sec: usize, para: usize, path: &Path, offset: u32) -> (f64, f64) {
    let rect: serde_json::Value = serde_json::from_str(
        &doc.get_cursor_rect_by_path(sec as u32, para as u32, &path_json(path), offset)
            .unwrap(),
    )
    .unwrap();
    let y = rect["y"].as_f64().unwrap();
    (y, y + rect["height"].as_f64().unwrap())
}

#[test]
fn nested_table_caption_is_read_and_edited_by_path() {
    let mut doc = load();
    let (sec, para, cell, direction) = nested_captioned_table(&doc);
    let caption = with_cell(&cell, CAPTION);
    let expected = nested_table(&doc, sec, para, &cell)
        .caption
        .as_ref()
        .unwrap()
        .paragraphs
        .len();

    assert_eq!(
        doc.get_cell_paragraph_count_by_path(sec as u32, para as u32, &path_json(&caption))
            .unwrap() as usize,
        expected
    );
    let len = doc
        .get_cell_paragraph_length_by_path(sec as u32, para as u32, &path_json(&caption))
        .unwrap() as usize;
    doc.insert_text_in_cell_by_path(sec, para, &caption, len, "AB")
        .unwrap();
    let text = doc
        .get_text_in_cell_by_path(sec, para, &caption, 0, usize::MAX / 2)
        .unwrap();
    assert!(text.ends_with("AB"), "중첩 표 캡션에 든다: {text:?}");

    // 캡션 캐럿은 그 표의 캡션 줄에서 찾는다: 위 캡션은 첫 칸 위, 아래 캡션은 마지막 칸 아래.
    let (top, bottom) = caret(&doc, sec, para, &caption, len as u32 + 2);
    let last_cell = nested_table(&doc, sec, para, &cell).cells.len() - 1;
    match direction {
        CaptionDirection::Top => {
            let (first_top, _) = caret(&doc, sec, para, &cell, 0);
            assert!(
                bottom <= first_top + 0.5,
                "캡션 캐럿 {top}..{bottom} 은 첫 칸 {first_top} 위"
            );
        }
        _ => {
            let (_, last_bottom) = caret(&doc, sec, para, &with_cell(&cell, last_cell), 0);
            assert!(
                top >= last_bottom - 0.5,
                "캡션 캐럿 {top}..{bottom} 은 마지막 칸 {last_bottom} 아래"
            );
        }
    }

    let reopened = HwpDocument::from_bytes(&doc.export_hwp_native().unwrap()).unwrap();
    let text = reopened
        .get_text_in_cell_by_path(sec, para, &caption, 0, usize::MAX / 2)
        .unwrap();
    assert!(text.ends_with("AB"), "저장 뒤에도 남는다: {text:?}");
}

#[test]
fn caption_step_without_caption_is_an_error() {
    let doc = load();
    // 바깥 표에는 캡션이 없다.
    let (sec, para, cell, _) = nested_captioned_table(&doc);
    let ci = cell[0].0;
    let Control::Table(outer) = &doc.document().sections[sec].paragraphs[para].controls[ci] else {
        unreachable!()
    };
    assert!(outer.caption.is_none(), "전제: 바깥 표에는 캡션이 없다");
    let error = doc
        .get_text_in_cell_by_path(sec, para, &[(ci, CAPTION, 0)], 0, 1)
        .unwrap_err();
    assert!(error.to_string().contains("캡션이 없습니다"), "{error}");
}

// 포크 전용: 만든 문서의 머리말 표 캡션 경로, 바깥 칸이 품는 칸 안 표 캡션, 머리말 영역을 넘친 캡션 히트.

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

/// 렌더 트리에서 글에 `text`가 든 글자 런 상자 (x, y, w, h)와, 그 런을 품은 가장 안쪽 칸 상자.
fn run_and_cell_box(doc: &HwpDocument, text: &str) -> ([f64; 4], Option<[f64; 4]>) {
    use serde_json::Value;
    fn rect(node: &Value) -> [f64; 4] {
        let b = |k: &str| node["bbox"][k].as_f64().unwrap();
        [b("x"), b("y"), b("w"), b("h")]
    }
    fn find(
        node: &Value,
        text: &str,
        cell: Option<[f64; 4]>,
    ) -> Option<([f64; 4], Option<[f64; 4]>)> {
        if node["type"] == "TextRun" && node["text"].as_str().is_some_and(|t| t.contains(text)) {
            return Some((rect(node), cell));
        }
        let cell = if node["type"] == "Cell" {
            Some(rect(node))
        } else {
            cell
        };
        node["children"]
            .as_array()?
            .iter()
            .find_map(|n| find(n, text, cell))
    }
    let tree: Value = serde_json::from_str(&doc.get_page_render_tree(0).unwrap()).unwrap();
    find(&tree, text, None).expect("글자 런")
}

/// 캡션을 아래에 두고 캡션 글 끝에 "AB"를 넣는다. 넣기 전 캡션 글 길이를 돌려준다.
fn caption_with_text(doc: &mut HwpDocument, root: usize, cell: &Path) -> usize {
    doc.set_table_properties_by_path_native(
        0,
        root,
        cell,
        r#"{"hasCaption":true,"captionDirection":3}"#,
    )
    .unwrap();
    let caption = with_cell(cell, CAPTION);
    let before = doc
        .get_cell_paragraph_length_by_path(0, root as u32, &path_json(&caption))
        .unwrap() as usize;
    doc.insert_text_in_cell_by_path(0, root, &caption, before, "AB")
        .unwrap();
    before
}

#[test]
fn caption_of_header_table_is_edited_by_path() {
    let (mut doc, _, _, host, header) = doc_with_inner_tables();
    let before = caption_with_text(&mut doc, host, &header);
    let caption = with_cell(&header, CAPTION);
    // 아래 캡션 캐럿은 그 표 마지막 줄 칸 캐럿보다 아래에 있다.
    let (top, _) = caret(&doc, 0, host, &caption, before as u32 + 2);
    let (_, row_bottom) = caret(&doc, 0, host, &with_cell(&header, 3), 0);
    assert!(
        top >= row_bottom,
        "캡션 캐럿 {top} / 마지막 줄 {row_bottom}"
    );
    for bytes in [
        doc.export_hwp_native().unwrap(),
        doc.export_hwpx_native().unwrap(),
    ] {
        let reopened = HwpDocument::from_bytes(&bytes).unwrap();
        let text = reopened
            .get_text_in_cell_by_path(0, host, &caption, 0, usize::MAX / 2)
            .unwrap();
        assert!(text.ends_with("AB"), "저장 뒤에도 남는다: {text:?}");
    }
}

#[test]
fn nested_table_caption_fits_in_outer_cell() {
    let (mut doc, body, nested, _, _) = doc_with_inner_tables();
    let before = caption_with_text(&mut doc, body, &nested);
    let ([_, y, _, h], cell) = run_and_cell_box(&doc, "AB");
    let [_, cell_y, _, cell_h] = cell.expect("캡션 글은 바깥 칸 안에 그린다");
    // 바깥 칸이 안쪽 표 캡션까지 품는다.
    assert!(
        cell_y + cell_h >= y + h - 0.5,
        "칸 {cell:?} / 캡션 줄 {y}+{h}"
    );
    // 캡션 캐럿은 칸 상자로 잘리지 않고 그 캡션 줄에 선다.
    let rect = json(
        &doc.get_cursor_rect_by_path(
            0,
            body as u32,
            &path_json(&with_cell(&nested, CAPTION)),
            before as u32 + 2,
        )
        .unwrap(),
    );
    let (cy, ch) = (
        rect["y"].as_f64().unwrap(),
        rect["height"].as_f64().unwrap(),
    );
    assert!(
        cy + ch >= y + h - 1.0 && cy <= y + 1.0,
        "캐럿 {rect} / 캡션 줄 {y}+{h}"
    );
}

#[test]
fn header_table_caption_below_header_area_is_a_header_hit_not_a_body_cell() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    doc.create_header_footer_native(0, true, 0).unwrap();
    let info = json(&doc.get_header_footer_native(0, true, 0).unwrap());
    let host = info["paraIndex"].as_u64().unwrap() as usize;
    let hf = info["controlIndex"].as_u64().unwrap() as usize;
    // 기본 4×4 표에 아래 캡션을 달면 캡션이 머리말 영역 아래 끝을 넘는다.
    let header = created_path(
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
    let before = caption_with_text(&mut doc, host, &header);
    let ([x, y, w, h], _) = run_and_cell_box(&doc, "AB");
    let (x, y) = (x + w - 0.5, y + h / 2.0);
    let area = &json(&doc.get_page_info_native(0).unwrap())["headerArea"];
    assert!(y > area["y"].as_f64().unwrap() + area["height"].as_f64().unwrap());

    let hit = json(&doc.hit_test_header_footer_native(0, x, y).unwrap());
    assert_eq!(
        (&hit["hit"], &hit["isHeader"]),
        (&json("true"), &json("true"))
    );
    // 본문 히트는 머리말 표 캡션을 (머리말 문단, 표 컨트롤, 65534) 본문 주소로 주지 않는다.
    let body = json(&doc.hit_test_native(0, x, y).unwrap());
    assert!(body.get("cellPath").is_none(), "{body}");
    // 머리말 편집 히트는 그 캡션 글 끝이다.
    let hit = json(
        &doc.hit_test_in_header_footer_target_native(0, 0, true, 0, x, y)
            .unwrap(),
    );
    assert_eq!(
        hit["cellPath"],
        json(&path_json(&with_cell(&header, CAPTION))),
        "{hit}"
    );
    assert_eq!(hit["charOffset"].as_u64(), Some(before as u64 + 2));
}
