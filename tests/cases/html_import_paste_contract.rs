//! HTML clipboard input is observed through the public native paste API.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::model::style::UnderlineType;
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};

const HTML_PASTE_MAX_BYTES: usize = 400_000;
const FLUSH_LINE_CHAR_CAP: usize = 4_000;

fn paste_html(html: &str) -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native()
        .expect("public blank document");
    core.paste_html_native(0, 0, 0, html)
        .expect("public HTML paste");
    core
}

fn paragraphs(core: &DocumentCore) -> &[rhwp::model::paragraph::Paragraph] {
    &core.document().sections[0].paragraphs
}

#[test]
fn pasted_tables_fit_the_column_and_paragraph_margins() {
    fn table_right(node: &RenderNode) -> Option<f64> {
        let own = matches!(node.node_type, RenderNodeType::Table(_))
            .then_some(node.bbox.x + node.bbox.width);
        node.children
            .iter()
            .filter_map(table_right)
            .fold(own, |a, b| Some(a.map_or(b, |a| a.max(b))))
    }
    for columns in [1, 2] {
        for table_style in ["", "width:150px", "width:5000px"] {
            let mut core = DocumentCore::new_empty();
            core.create_blank_document_native().expect("blank");
            core.set_page_def_native(0, r#"{"width":30000,"marginLeft":4000,"marginRight":4000}"#)
                .expect("narrow paper");
            core.set_column_def_native(0, columns, 0, true, 600)
                .expect("columns");
            let shape = &mut core.document_mut().doc_info.para_shapes[1];
            shape.margin_left = 3000;
            shape.margin_right = 1500;
            shape.raw_data = None;
            core.paste_html_native(0, 0, 0, &format!(
                r#"<p>앞</p><table style="{table_style}"><tr><td>셀</td></tr></table><p>뒤</p>"#,
            )).expect("mixed HTML paste");
            let table = paragraphs(&core)
                .iter()
                .flat_map(|p| &p.controls)
                .find_map(|c| match c {
                    Control::Table(t) => Some(t),
                    _ => None,
                })
                .expect("table");
            if table_style == "width:150px" && columns == 1 {
                assert_eq!(table.common.width, 11250, "작게 지정한 표 폭을 유지한다");
            }
            let column_right = (30000 - 4000) as f64 * 96.0 / 7200.0;
            let column_right = if columns == 2 {
                4000.0 * 96.0 / 7200.0 + (22000.0 - 600.0) / 2.0 * 96.0 / 7200.0
            } else {
                column_right
            };
            for bytes in [
                core.export_hwp_native().expect("HWP"),
                core.export_hwpx_native().expect("HWPX"),
            ] {
                let reopened = DocumentCore::from_bytes(&bytes).expect("reopen");
                let mut found = false;
                for page in 0..reopened.page_count() {
                    let tree = reopened.build_page_render_tree(page).expect("page");
                    if let Some(right) = table_right(&tree.root) {
                        found = true;
                        assert!(
                            right <= column_right + 0.5,
                            "{columns}단 {table_style}: 표 우단 {right} > 단 우단 {column_right}"
                        );
                    }
                }
                assert!(found, "저장 후에도 표를 그린다");
            }
        }
    }
}

#[test]
fn nested_html_tables_use_the_rendered_padding_and_reject_missing_space() {
    for padding in ["", "padding:20px", "padding:500px"] {
        let mut core = DocumentCore::new_empty();
        core.create_blank_document_native().expect("blank");
        let before = core.export_hwp_native().expect("before");
        let html = format!(
            r#"<p style="font-weight:bold">앞</p><table style="width:200px;{padding}"><tr><td>바깥<table><tr><td>안쪽</td></tr></table></td></tr></table>"#
        );
        let result = core.paste_html_native(0, 0, 0, &html);
        if padding == "padding:500px" {
            assert!(result.is_err(), "안쪽 폭이 없으면 원문을 보존하고 거절한다");
            assert_eq!(core.export_hwp_native().expect("after"), before);
            continue;
        }
        result.expect("nested paste");
        for bytes in [
            core.export_hwp_native().expect("HWP"),
            core.export_hwpx_native().expect("HWPX"),
        ] {
            let reopened = DocumentCore::from_bytes(&bytes).expect("reopen");
            let outer = paragraphs(&reopened)
                .iter()
                .flat_map(|p| &p.controls)
                .find_map(|c| match c {
                    Control::Table(t) => Some(t),
                    _ => None,
                })
                .expect("outer");
            let nested = outer.cells[0]
                .paragraphs
                .iter()
                .flat_map(|p| &p.controls)
                .find_map(|c| match c {
                    Control::Table(t) => Some(t),
                    _ => None,
                })
                .expect("inner");
            let padding_width = if padding.is_empty() { 1020 } else { 3000 };
            assert!(
                nested.common.width
                    + nested.outer_margin_left as u32
                    + nested.outer_margin_right as u32
                    <= 15000 - padding_width
            );
            assert!(
                nested.cells[0].paragraphs.iter().any(|p| p.text == "안쪽"),
                "저장 후에도 안쪽 글을 남긴다"
            );
        }
    }
}

#[test]
fn top_level_span_paste_keeps_inline_text_and_styles_without_raw_tags() {
    let core =
        paste_html("<span style=\"color:#ff0000\"><strong>홍길동</strong><u> 부장</u></span>");
    let paragraph = &paragraphs(&core)[0];

    assert_eq!(paragraph.text, "홍길동 부장");
    assert!(
        !paragraph.text.contains('<'),
        "최상위 span 내부 태그가 문서 문자로 남으면 안 된다"
    );

    let applied_shapes: Vec<_> = paragraph
        .char_shapes
        .iter()
        .map(|run| &core.document().doc_info.char_shapes[run.char_shape_id as usize])
        .collect();
    assert!(
        applied_shapes.iter().any(|shape| shape.bold),
        "중첩 strong 서식이 붙여넣은 문단에 적용돼야 함"
    );
    assert!(
        applied_shapes
            .iter()
            .any(|shape| matches!(shape.underline_type, UnderlineType::Bottom)),
        "중첩 u 서식이 붙여넣은 문단에 적용돼야 함"
    );
}

#[test]
fn list_item_paste_becomes_bulleted_paragraphs() {
    let core = paste_html("<ul><li>첫 번째 <strong>항목</strong></li><li>둘째 항목</li></ul>");
    let texts: Vec<_> = paragraphs(&core)
        .iter()
        .map(|paragraph| paragraph.text.as_str())
        .collect();

    assert_eq!(texts, ["• 첫 번째 항목", "• 둘째 항목"]);
}

#[test]
fn long_plain_text_paste_is_split_before_layout() {
    let core = paste_html(&"가".repeat(FLUSH_LINE_CHAR_CAP * 2 + 1));
    let paragraphs = paragraphs(&core);

    assert_eq!(paragraphs.len(), 3);
    assert_eq!(paragraphs[0].text.chars().count(), FLUSH_LINE_CHAR_CAP);
    assert_eq!(paragraphs[1].text.chars().count(), FLUSH_LINE_CHAR_CAP);
    assert_eq!(paragraphs[2].text, "가");
}

#[test]
fn oversized_markup_paste_falls_back_to_capped_paragraphs() {
    let text = "가".repeat(HTML_PASTE_MAX_BYTES + 1);
    let core = paste_html(&format!("<div>{text}</div>"));
    let paragraphs = paragraphs(&core);

    assert_eq!(paragraphs.len(), 101);
    assert!(paragraphs
        .iter()
        .all(|paragraph| paragraph.text.chars().count() <= FLUSH_LINE_CHAR_CAP));
    assert_eq!(paragraphs.last().expect("마지막 문단").text, "가");
}
