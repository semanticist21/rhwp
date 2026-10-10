//! 셀 선택과 표 HTML 복사는 강제 줄바꿈·공백·같은 문자 구간의 굵게를 보존한다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::model::paragraph::Paragraph;
use rhwp::model::table::Table;
use serde_json::Value;

const TEXT: &str = "  앞<&>\n뒤  ";
const MARKS: &str = "..BBBBBB..";
const SIBLING: &str = "형제 셀";

struct Fixture {
    core: DocumentCore,
    parent: usize,
    control: usize,
}

fn table() -> Fixture {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    let created: Value =
        serde_json::from_str(&core.create_table_native(0, 0, 0, 1, 2).unwrap()).unwrap();
    let parent = created["paraIdx"].as_u64().unwrap() as usize;
    let control = created["controlIdx"].as_u64().unwrap() as usize;
    core.insert_text_in_cell_native(0, parent, control, 1, 0, 0, SIBLING)
        .unwrap();
    Fixture {
        core,
        parent,
        control,
    }
}

fn source() -> Fixture {
    let mut fixture = table();
    fixture
        .core
        .insert_text_in_cell_native(0, fixture.parent, fixture.control, 0, 0, 0, TEXT)
        .unwrap();
    fixture
        .core
        .apply_char_format_in_cell_native(
            0,
            fixture.parent,
            fixture.control,
            0,
            0,
            2,
            8,
            r#"{"bold":true}"#,
        )
        .unwrap();
    assert_cells(&fixture.core);
    fixture
}

fn cell(core: &DocumentCore, index: usize) -> &Paragraph {
    core.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|para| &para.controls)
        .find_map(|control| match control {
            Control::Table(table) => {
                assert_eq!(
                    table.cells[index].paragraphs.len(),
                    1,
                    "LF는 같은 문단의 강제 줄바꿈이다"
                );
                Some(&table.cells[index].paragraphs[0])
            }
            _ => None,
        })
        .unwrap()
}

fn assert_cells(core: &DocumentCore) {
    let para = cell(core, 0);
    assert_eq!(para.text, TEXT, "강제 LF와 앞뒤 두 공백을 정확히 유지한다");
    let marks: String = (0..para.text.chars().count())
        .map(|index| {
            let id = para.char_shape_id_at(index).unwrap();
            if core.document().doc_info.char_shapes[id as usize].bold {
                'B'
            } else {
                '.'
            }
        })
        .collect();
    assert_eq!(marks, MARKS, "LF 앞뒤 문자는 같은 굵은 구간이다");
    let sibling = cell(core, 1);
    assert_eq!(sibling.text, SIBLING);
    assert!(
        (0..SIBLING.chars().count()).all(|index| {
            !core.document().doc_info.char_shapes[sibling.char_shape_id_at(index).unwrap() as usize]
                .bold
        }),
        "형제 셀의 직접 서식을 바꾸지 않는다"
    );
}

fn assert_saved(core: &DocumentCore) {
    assert_cells(core);
    for (format, bytes) in [
        ("HWP", core.export_hwp_native().unwrap()),
        ("HWPX", core.export_hwpx_native().unwrap()),
    ] {
        let reopened = DocumentCore::from_bytes(&bytes).unwrap();
        assert_cells(&reopened);
        assert_eq!(reopened.page_count(), core.page_count(), "{format} 쪽 수");
    }
}

fn clipboard_html(html: &str) -> String {
    assert!(
        html.contains("&lt;&amp;&gt;"),
        "사용자 태그 모양은 HTML로 해석하지 않는다"
    );
    // 앱처럼 바깥 공백만 제거하고, 중첩 블록 사이 출력 개행은 그대로 전달한다.
    html.trim().to_string()
}

#[test]
fn selected_cell_html_keeps_forced_break_spaces_and_bold_in_both_formats() {
    let source = source();
    let html = source
        .core
        .export_selection_in_cell_html_native(
            0,
            source.parent,
            source.control,
            0,
            0,
            0,
            0,
            TEXT.chars().count(),
        )
        .unwrap();
    let mut target = table();
    target
        .core
        .paste_html_in_cell_native(
            0,
            target.parent,
            target.control,
            0,
            0,
            0,
            &clipboard_html(&html),
        )
        .unwrap();
    assert_saved(&target.core);
    assert_eq!(html.matches("<br>").count(), 1);
    assert_saved(&source.core);
}

#[test]
fn table_html_keeps_forced_break_spaces_and_bold_in_both_formats() {
    let source = source();
    let html = source
        .core
        .export_control_html_native(0, source.parent, &[], source.control)
        .unwrap();
    let mut target = DocumentCore::new_empty();
    target.create_blank_document_native().unwrap();
    target
        .paste_html_native(0, 0, 0, &clipboard_html(&html))
        .unwrap();
    assert_saved(&target);
    assert_eq!(html.matches("<br>").count(), 1);
    assert_saved(&source.core);
}

fn top_table(core: &DocumentCore) -> (usize, usize, &Table) {
    core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .find_map(|(parent, para)| {
            para.controls
                .iter()
                .enumerate()
                .find_map(|(control, item)| match item {
                    Control::Table(table) => Some((parent, control, table.as_ref())),
                    _ => None,
                })
        })
        .unwrap()
}

fn saved_documents(core: &DocumentCore) -> [DocumentCore; 2] {
    [
        DocumentCore::from_bytes(&core.export_hwp_native().unwrap()).unwrap(),
        DocumentCore::from_bytes(&core.export_hwpx_native().unwrap()).unwrap(),
    ]
}

fn paste_table(source: &DocumentCore) -> DocumentCore {
    let (parent, control, _) = top_table(source);
    let html = source
        .export_control_html_native(0, parent, &[], control)
        .unwrap();
    let mut target = DocumentCore::new_empty();
    target.create_blank_document_native().unwrap();
    target.paste_html_native(0, 0, 0, html.trim()).unwrap();
    target
}

const EXACT_CELLS: [&str; 4] = ["  앞굵게뒤  ", "", "   ", "  첫\n둘  "];

fn assert_exact_cells(core: &DocumentCore) {
    let (_, _, table) = top_table(core);
    assert_eq!(table.cells.len(), EXACT_CELLS.len());
    for (index, expected) in EXACT_CELLS.iter().enumerate() {
        assert_eq!(table.cells[index].paragraphs.len(), 1, "셀 {index} 문단 수");
        assert_eq!(
            table.cells[index].paragraphs[0].text, *expected,
            "셀 {index} 원문"
        );
    }
    let para = &table.cells[0].paragraphs[0];
    for index in 0..EXACT_CELLS[0].chars().count() {
        let shape =
            &core.document().doc_info.char_shapes[para.char_shape_id_at(index).unwrap() as usize];
        assert_eq!(shape.bold, (3..5).contains(&index), "글자 {index} 굵게");
        assert_eq!(shape.italic, index == 5, "글자 {index} 기울임");
    }
}

#[test]
fn table_html_keeps_blank_space_only_and_exact_styled_cell_text() {
    let mut source = DocumentCore::new_empty();
    source.create_blank_document_native().unwrap();
    let created: Value =
        serde_json::from_str(&source.create_table_native(0, 0, 0, 1, 4).unwrap()).unwrap();
    let parent = created["paraIdx"].as_u64().unwrap() as usize;
    let control = created["controlIdx"].as_u64().unwrap() as usize;
    for (index, text) in EXACT_CELLS.iter().enumerate() {
        if !text.is_empty() {
            source
                .insert_text_in_cell_native(0, parent, control, index, 0, 0, text)
                .unwrap();
        }
    }
    source
        .apply_char_format_in_cell_native(0, parent, control, 0, 0, 3, 5, r#"{"bold":true}"#)
        .unwrap();
    source
        .apply_char_format_in_cell_native(0, parent, control, 0, 0, 5, 6, r#"{"italic":true}"#)
        .unwrap();
    assert_exact_cells(&source);
    // 두 원본 형식 각각의 writer 출력과, 붙인 뒤 두 형식 저장을 독립적으로 검사한다.
    for source in saved_documents(&source) {
        assert_exact_cells(&source);
        let target = paste_table(&source);
        assert_exact_cells(&target);
        for reopened in saved_documents(&target) {
            assert_exact_cells(&reopened);
        }
        assert_exact_cells(&source);
    }
}

fn count_tables(paragraphs: &[Paragraph]) -> usize {
    paragraphs
        .iter()
        .flat_map(|para| &para.controls)
        .map(|control| match control {
            Control::Table(table) => {
                1 + table
                    .cells
                    .iter()
                    .map(|cell| count_tables(&cell.paragraphs))
                    .sum::<usize>()
            }
            _ => 0,
        })
        .sum()
}

fn assert_nested_table(core: &DocumentCore) {
    assert_eq!(count_tables(&core.document().sections[0].paragraphs), 2);
    let (_, _, outer) = top_table(core);
    assert_eq!(outer.cells.len(), 2);
    assert_eq!(outer.cells[0].paragraphs.len(), 3);
    assert_eq!(outer.cells[0].paragraphs[0].text, "바깥 셀");
    assert_eq!(outer.cells[0].paragraphs[1].text, "");
    assert_eq!(outer.cells[0].paragraphs[2].text, "바깥 뒤");
    assert_eq!(outer.cells[1].paragraphs.len(), 1);
    assert_eq!(outer.cells[1].paragraphs[0].text, "이웃 셀");
    let Control::Table(inner) = &outer.cells[0].paragraphs[1].controls[0] else {
        panic!("가운데 문단은 하위 표를 소유한다");
    };
    assert_eq!(inner.cells.len(), 1);
    assert_eq!(inner.cells[0].paragraphs.len(), 1);
    assert_eq!(inner.cells[0].paragraphs[0].text, "안쪽 셀🦦");
}

#[test]
fn table_html_keeps_nested_tables_and_each_cell_once_without_minifying() {
    let mut source = DocumentCore::new_empty();
    source.create_blank_document_native().unwrap();
    source.paste_html_native(0, 0, 0,
        "<p>본문 앞</p><table><tr><td><p>바깥 셀</p><table><tr><td>안쪽 셀🦦</td></tr></table><p>바깥 뒤</p></td><td>이웃 셀</td></tr></table><p>본문 뒤</p>",
    ).unwrap();
    assert_nested_table(&source);
    for source in saved_documents(&source) {
        assert_nested_table(&source);
        let target = paste_table(&source);
        assert_nested_table(&target);
        for reopened in saved_documents(&target) {
            assert_nested_table(&reopened);
        }
        assert_nested_table(&source);
    }
}

#[test]
fn empty_or_block_formatting_only_external_cells_stay_blank() {
    let mut source = DocumentCore::new_empty();
    source.create_blank_document_native().unwrap();
    source.paste_html_native(0, 0, 0,
        "<table>\n<tr><td></td><td> \n\t </td><td><p></p></td><td>\n<p><span></span></p>\n</td></tr>\n</table>",
    ).unwrap();
    let saved = saved_documents(&source);
    for core in std::iter::once(source).chain(saved) {
        let (_, _, table) = top_table(&core);
        assert_eq!(table.cells.len(), 4);
        for cell in &table.cells {
            assert_eq!(cell.paragraphs.len(), 1);
            assert_eq!(cell.paragraphs[0].text, "");
            assert!(cell.paragraphs[0].controls.is_empty());
        }
    }
}
