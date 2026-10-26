//! 인라인 표를 HTML로 옮길 때 원문 문단과 글자 앵커를 보존한다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::model::paragraph::Paragraph;
use rhwp::model::style::{Alignment, LineSpacingType, UnderlineType};
use rhwp::model::table::Table;
use serde_json::Value;

const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j0ioAAAAASUVORK5CYII=";
const TEXT: &str = "  가🦦나\n다끝  ";

fn blank() -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core
}

fn saved(core: &DocumentCore) -> [DocumentCore; 2] {
    [
        DocumentCore::from_bytes(&core.export_hwp_native().unwrap()).unwrap(),
        DocumentCore::from_bytes(&core.export_hwpx_native().unwrap()).unwrap(),
    ]
}

fn first_table(core: &DocumentCore) -> (usize, usize, &Table) {
    core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .find_map(|(para, paragraph)| {
            paragraph
                .controls
                .iter()
                .enumerate()
                .find_map(|(index, control)| {
                    if let Control::Table(table) = control {
                        Some((para, index, table.as_ref()))
                    } else {
                        None
                    }
                })
        })
        .unwrap()
}

fn inline_html() -> String {
    format!(
        r#"<div data-rhwp-paragraph="inline" style="text-align:right;font-size:16pt"><span>  <b>가🦦</b></span><table style="display:inline-table;width:20.03pt;height:12.07pt"><tr><td style="width:20.03pt;height:12.07pt"><p>첫 표</p></td></tr></table><i>나</i><img src="{PNG}" width="22.68" height="19.106666666666666"><br><u>다</u><table style="display:inline-table;width:30.05pt;height:15.09pt"><tr><td style="width:30.05pt;height:15.09pt"><p>둘 표</p></td></tr></table><span>끝  </span></div>"#
    )
}

fn assert_inline(core: &DocumentCore, paragraph: &Paragraph, exact_dimensions: bool) {
    assert_eq!(paragraph.text, TEXT);
    let table_ids: Vec<_> = paragraph
        .controls
        .iter()
        .filter_map(|control| match control {
            Control::Table(table) => Some(table.common.instance_id),
            _ => None,
        })
        .collect();
    assert!(table_ids.iter().all(|id| *id != 0));
    let mut unique_ids = table_ids.clone();
    unique_ids.sort_unstable();
    unique_ids.dedup();
    assert_eq!(
        unique_ids.len(),
        table_ids.len(),
        "같은 문단의 표는 서로 다른 개체 번호를 소유한다"
    );
    assert_eq!(
        core.document().doc_info.para_shapes[paragraph.para_shape_id as usize].alignment,
        Alignment::Right
    );
    let positions = paragraph.control_text_positions();
    let objects: Vec<_> = paragraph
        .controls
        .iter()
        .zip(positions)
        .filter_map(|(control, at)| match control {
            Control::Table(table) => {
                assert!(table.common.treat_as_char);
                assert_eq!(table.cells.len(), 1);
                assert_eq!(table.cells[0].paragraphs.len(), 1);
                if exact_dimensions {
                    // 선언 높이(1207·1509)보다 내용이 길면 한컴처럼 표 높이만 내용에 맞춰 커진다.
                    let expected = if at == 4 { (2003, 2882) } else { (3005, 2882) };
                    assert_eq!((table.common.width, table.common.height), expected);
                }
                Some(("table", at, table.cells[0].paragraphs[0].text.clone()))
            }
            Control::Picture(picture) => {
                assert!(picture.common.treat_as_char);
                if exact_dimensions {
                    assert_eq!((picture.common.width, picture.common.height), (1701, 1433));
                }
                Some(("picture", at, String::new()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        objects,
        [
            ("table", 4, "첫 표".to_string()),
            ("picture", 5, String::new()),
            ("table", 7, "둘 표".to_string()),
        ]
    );
    for index in 0..TEXT.chars().count() {
        let shape = &core.document().doc_info.char_shapes
            [paragraph.char_shape_id_at(index).unwrap() as usize];
        assert_eq!(shape.bold, (2..4).contains(&index), "글자 {index} 굵게");
        assert_eq!(shape.italic, index == 4, "글자 {index} 기울임");
        assert_eq!(
            shape.underline_type == UnderlineType::Bottom,
            index == 6,
            "글자 {index} 밑줄"
        );
        assert_eq!(shape.base_size, 1600, "문단 글자 크기 상속 {index}");
    }
}

#[test]
fn marked_body_inline_tables_keep_text_anchors_order_styles_and_saved_formats() {
    let mut core = blank();
    core.paste_html_native(0, 0, 0, &inline_html()).unwrap();
    assert_eq!(core.document().sections[0].paragraphs.len(), 1);
    assert_inline(&core, &core.document().sections[0].paragraphs[0], true);
    for reopened in saved(&core) {
        assert_eq!(reopened.document().sections[0].paragraphs.len(), 1);
        assert_inline(
            &reopened,
            &reopened.document().sections[0].paragraphs[0],
            true,
        );
    }
    let mut prior = blank();
    prior
        .paste_html_native(
            0,
            0,
            0,
            &format!("<table><tr><td>앞선 표</td></tr></table>{}", inline_html()),
        )
        .unwrap();
    for result in std::iter::once(&prior).chain(saved(&prior).iter()) {
        let ids: Vec<_> = result.document().sections[0]
            .paragraphs
            .iter()
            .flat_map(|para| &para.controls)
            .filter_map(|control| match control {
                Control::Table(table) => Some(table.common.instance_id),
                _ => None,
            })
            .collect();
        assert_eq!(ids.len(), 3);
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            unique.len(),
            3,
            "앞선 문단의 표 번호도 marker 안 표에서 재사용하지 않는다"
        );
    }
}

#[test]
fn whole_table_export_keeps_inline_tables_in_one_nested_cell_paragraph() {
    for nested in [false, true] {
        let content = format!("<p></p>{}<p></p><p>끝<br>줄</p>", inline_html());
        let inner = format!(
            r#"<table style="width:200pt"><tr><td>{content}</td><td><p>이웃 셀</p></td></tr></table>"#
        );
        let html = if nested {
            format!(r#"<table style="width:300pt"><tr><td>{inner}</td></tr></table>"#)
        } else {
            inner
        };
        let mut core = blank();
        core.paste_html_native(0, 0, 0, &html).unwrap();
        for source in saved(&core) {
            let (para, control, _) = first_table(&source);
            let copied = source
                .export_control_html_native(0, para, &[], control)
                .unwrap();
            assert_eq!(
                copied.matches("data-rhwp-paragraph=\"inline\"").count(),
                if nested { 2 } else { 1 }
            );
            assert_eq!(
                copied.matches("display:inline-table;").count(),
                if nested { 3 } else { 2 }
            );
            let mut target = blank();
            target.paste_html_native(0, 0, 0, &copied).unwrap();
            let reopened = saved(&target);
            for result in std::iter::once(target).chain(reopened) {
                let (_, _, outer) = first_table(&result);
                let table = if nested {
                    assert_eq!(outer.cells[0].paragraphs.len(), 1);
                    let Control::Table(table) = &outer.cells[0].paragraphs[0].controls[0] else {
                        panic!("중첩 표");
                    };
                    table.as_ref()
                } else {
                    outer
                };
                let paragraphs = &table.cells[0].paragraphs;
                assert_eq!(paragraphs.len(), 4);
                assert_eq!(paragraphs[0].text, "");
                assert!(paragraphs[0].controls.is_empty());
                assert_inline(&result, &paragraphs[1], false);
                assert_eq!(paragraphs[2].text, "");
                assert!(paragraphs[2].controls.is_empty());
                assert_eq!(paragraphs[3].text, "끝\n줄");
                assert_eq!(table.cells[1].paragraphs[0].text, "이웃 셀");
            }
        }
    }
}

#[test]
fn public_model_whole_table_export_preserves_inline_table_anchor() {
    let mut core = blank();
    core.insert_text_native(0, 0, 0, "앞🦦뒤").unwrap();
    let inner: Value = serde_json::from_str(
        &core
            .create_table_ex_native(0, 0, 2, 1, 1, true, Some(&[2003]), Some(&[1207]))
            .unwrap(),
    )
    .unwrap();
    core.insert_text_in_cell_native(
        0,
        0,
        inner["controlIdx"].as_u64().unwrap() as usize,
        0,
        0,
        0,
        "안쪽",
    )
    .unwrap();
    core.insert_paragraph_native(0, 1).unwrap();
    let outer: Value = serde_json::from_str(
        &core
            .create_table_ex_native(0, 1, 0, 1, 1, true, Some(&[20003]), Some(&[8007]))
            .unwrap(),
    )
    .unwrap();
    core.copy_selection_native(0, 0, 0, 0, 3).unwrap();
    core.paste_internal_in_cell_native(
        0,
        1,
        outer["controlIdx"].as_u64().unwrap() as usize,
        0,
        0,
        0,
    )
    .unwrap();
    let copied = core
        .export_control_html_native(0, 1, &[], outer["controlIdx"].as_u64().unwrap() as usize)
        .unwrap();
    let mut target = blank();
    target.paste_html_native(0, 0, 0, &copied).unwrap();
    let reopened = saved(&target);
    for result in std::iter::once(target).chain(reopened) {
        let outer = result.document().sections[0]
            .paragraphs
            .iter()
            .flat_map(|para| &para.controls)
            .filter_map(|control| match control {
                Control::Table(table) if table.cells[0].paragraphs[0].text == "앞🦦뒤" => {
                    Some(table)
                }
                _ => None,
            })
            .next()
            .unwrap();
        assert_eq!(outer.cells[0].paragraphs.len(), 1);
        let para = &outer.cells[0].paragraphs[0];
        assert_eq!(para.control_text_positions(), [2]);
        let Control::Table(inner) = &para.controls[0] else {
            panic!("안쪽 표");
        };
        assert!(inner.common.treat_as_char);
        assert_eq!(inner.cells[0].paragraphs[0].text, "안쪽");
    }
}

#[test]
fn ordinary_and_malformed_inline_paragraph_markers_keep_block_boundaries() {
    for div in [
        "<div>",
        "<div data-rhwp-paragraph=\"other\">",
        "<div data-rhwp-paragraph=\"inline\">",
    ] {
        let mut core = blank();
        core.paste_html_native(
            0,
            0,
            0,
            &format!("{div}<p>앞</p><table><tr><td>표</td></tr></table><p>뒤</p></div>"),
        )
        .unwrap();
        let paras = &core.document().sections[0].paragraphs;
        assert_eq!(paras.len(), 3);
        assert_eq!(paras[0].text, "앞");
        assert_eq!(paras[1].controls.len(), 1);
        assert_eq!(paras[2].text, "뒤");
    }
    let mut core = blank();
    let before = core.export_hwp_native().unwrap();
    assert!(core.paste_html_native(0, 0, 0, r#"<div data-rhwp-paragraph="inline"><span>앞</span><table></table><span>뒤</span></div>"#).is_err());
    assert_eq!(core.export_hwp_native().unwrap(), before);
}

#[test]
fn marked_table_at_start_keeps_alignment_without_changing_a_nonempty_host() {
    for alignment in ["center", "right"] {
        for text in ["", "뒤"] {
            let html = format!(
                r#"<html><body><!--StartFragment--><!--sudal-clipboard:test--><div data-rhwp-paragraph="inline" style="text-align:{alignment}"><table><tr><td>표</td></tr></table><span>{text}</span></div><!--EndFragment--></body></html>"#
            );
            let mut source = blank();
            source
                .set_page_def_native(
                    0,
                    r#"{"width":50003,"height":70007,"marginLeft":2001,"marginRight":2003}"#,
                )
                .unwrap();
            let page = source.get_page_def_native(0).unwrap();
            source.paste_html_native(0, 0, 0, &html).unwrap();
            for result in std::iter::once(&source).chain(saved(&source).iter()) {
                let para = &result.document().sections[0].paragraphs[0];
                assert_eq!(para.text, text);
                assert_eq!(result.get_page_def_native(0).unwrap(), page);
                assert_eq!(
                    result.document().doc_info.para_shapes[para.para_shape_id as usize].alignment,
                    if alignment == "right" {
                        Alignment::Right
                    } else {
                        Alignment::Center
                    }
                );
                let prefix = result.export_selection_html_native(0, 0, 0, 0, 0).unwrap();
                assert!(
                    prefix.contains(&format!("text-align:{alignment}")),
                    "유효 빈 범위에도 문단 CSS를 내보낸다"
                );
            }
            for offset in [0, 1] {
                let mut target = blank();
                target.insert_text_native(0, 0, 0, "기존").unwrap();
                target
                    .apply_para_format_native(0, 0, r#"{"alignment":"left"}"#)
                    .unwrap();
                target.paste_html_native(0, 0, offset, &html).unwrap();
                for result in std::iter::once(&target).chain(saved(&target).iter()) {
                    let paras = &result.document().sections[0].paragraphs;
                    let para = &paras[0];
                    assert_eq!(
                        result.document().doc_info.para_shapes[para.para_shape_id as usize]
                            .alignment,
                        Alignment::Left,
                        "비빈 호스트 앞/중간은 원문 문단 모양을 유지한다"
                    );
                    assert_eq!(
                        paras
                            .iter()
                            .map(|para| para.text.as_str())
                            .collect::<String>(),
                        if offset == 0 {
                            format!("{text}기존")
                        } else {
                            format!("기{text}존")
                        }
                    );
                }
            }
        }
    }
    let html = r#"<div data-rhwp-paragraph="inline" style="text-align:right"><table><tr><td>표</td></tr></table></div><p style="text-align:right"></p>"#;
    for offset in [0, 1] {
        let mut core = blank();
        core.insert_text_native(0, 0, 0, "기존글").unwrap();
        core.apply_para_format_native(
            0,
            0,
            r#"{"alignment":"left","marginLeft":301,"lineSpacing":173}"#,
        )
        .unwrap();
        let old_shape = core.document().doc_info.para_shapes
            [core.document().sections[0].paragraphs[0].para_shape_id as usize]
            .clone();
        core.paste_html_native(0, 0, offset, html).unwrap();
        for result in std::iter::once(&core).chain(saved(&core).iter()) {
            let paras = &result.document().sections[0].paragraphs;
            assert_eq!(paras.len(), 2);
            assert_eq!(paras[1].text, if offset == 0 { "기존글" } else { "존글" });
            let shape = &result.document().doc_info.para_shapes[paras[1].para_shape_id as usize];
            assert_eq!(shape.alignment, old_shape.alignment);
            assert_eq!(shape.margin_left, old_shape.margin_left);
            assert_eq!(shape.line_spacing, old_shape.line_spacing);
        }
    }
}

#[test]
fn marked_paragraph_table_width_respects_its_own_margins_and_rolls_back_no_space() {
    // 새 표는 한컴처럼 바깥 여백 283×2를 둔다. 남은 폭이 "표" 한 글자 칸(1992)보다 넓도록
    // 문단 여백 390pt를 쓴다.
    for margin in ["390pt", "500pt"] {
        let mut core = blank();
        let before = core.export_hwp_native().unwrap();
        let html = format!(
            r#"<div data-rhwp-paragraph="inline" style="margin-left:{margin}"><span>앞</span><table style="width:100pt"><tr><td>표</td></tr></table><span>뒤</span></div>"#
        );
        let result = core.paste_html_native(0, 0, 0, &html);
        if margin == "500pt" {
            assert!(result.is_err());
            assert_eq!(core.export_hwp_native().unwrap(), before);
        } else {
            result.unwrap();
            for saved in std::iter::once(&core).chain(saved(&core).iter()) {
                let (_, _, table) = first_table(saved);
                assert!(
                    table.common.width
                        + table.outer_margin_left as u32
                        + table.outer_margin_right as u32
                        <= 42520 - 39000,
                    "표 폭 {} + 바깥 여백 {} {}",
                    table.common.width,
                    table.outer_margin_left,
                    table.outer_margin_right
                );
                assert_eq!(saved.document().sections[0].paragraphs[0].text, "앞뒤");
            }
        }
    }
}

#[test]
fn quoted_closing_tag_in_marker_attribute_never_panics() {
    for title in ["</div>", "İ</div>"] {
        let mut core = blank();
        let html = format!(
            r#"<div data-rhwp-paragraph="inline" title="{title}"><table><tr><td>표</td></tr></table></div>"#
        );
        core.paste_html_native(0, 0, 0, &html).unwrap();
        assert_eq!(first_table(&core).2.cells[0].paragraphs[0].text, "표");
    }
    let mut core = blank();
    let title = "İ".repeat(100);
    core.paste_html_native(0, 0, 0, &format!(r#"<div title="{title}" data-rhwp-paragraph="inline"><table><tr><td>표</td></tr></table><span>뒤</span></div>"#)).unwrap();
    assert_eq!(core.document().sections[0].paragraphs.len(), 1);
    assert_eq!(core.document().sections[0].paragraphs[0].text, "뒤");
    assert_eq!(first_table(&core).2.cells[0].paragraphs[0].text, "표");
}

fn leaf_paragraphs(paras: &[Paragraph]) -> &[Paragraph] {
    paras
        .iter()
        .find_map(|para| {
            if para
                .controls
                .iter()
                .any(|control| matches!(control, Control::Picture(_)))
            {
                Some(paras)
            } else {
                para.controls.iter().find_map(|control| match control {
                    Control::Table(table) => Some(leaf_paragraphs(&table.cells[0].paragraphs)),
                    _ => None,
                })
            }
        })
        .unwrap()
}

#[test]
fn eight_inline_tables_keep_the_innermost_picture_break_and_empty_paragraph() {
    let mut html =
        format!(r#"<p><b>가</b><img src="{PNG}" width="20" height="24">끝<br>줄</p><p></p>"#);
    for depth in 0..8 {
        html = format!(
            r#"<div data-rhwp-paragraph="inline"><span>앞{depth}</span><table><tr><td>{html}</td></tr></table><span>뒤{depth}</span></div>"#
        );
    }
    let mut core = blank();
    core.paste_html_native(0, 0, 0, &html).unwrap();
    for source in saved(&core) {
        let (para, control, _) = first_table(&source);
        let copied = source
            .export_control_html_native(0, para, &[], control)
            .unwrap();
        assert_eq!(copied.matches("<table ").count(), 8);
        let mut target = blank();
        target.paste_html_native(0, 0, 0, &copied).unwrap();
        for result in std::iter::once(&target).chain(saved(&target).iter()) {
            let paragraphs = leaf_paragraphs(&result.document().sections[0].paragraphs);
            assert_eq!(paragraphs.len(), 2);
            assert_eq!(paragraphs[1].text, "");
            assert!(paragraphs[1].controls.is_empty());
            let leaf = &paragraphs[0];
            assert_eq!(leaf.text, "가끝\n줄");
            assert_eq!(leaf.control_text_positions(), [1]);
            let Control::Picture(pic) = &leaf.controls[0] else {
                panic!("가장 깊은 그림");
            };
            assert_eq!((pic.common.width, pic.common.height), (1500, 1800));
            assert!(pic.common.treat_as_char);
        }
    }
}

#[test]
fn floating_table_only_cell_keeps_legacy_block_export_without_empty_prefix() {
    let mut core = blank();
    let inner: Value = serde_json::from_str(
        &core
            .create_table_ex_native(0, 0, 0, 1, 1, false, None, None)
            .unwrap(),
    )
    .unwrap();
    core.insert_paragraph_native(0, 1).unwrap();
    let outer: Value = serde_json::from_str(
        &core
            .create_table_ex_native(0, 1, 0, 1, 1, true, Some(&[20003]), None)
            .unwrap(),
    )
    .unwrap();
    core.copy_control_native(
        0,
        inner["paraIdx"].as_u64().unwrap() as usize,
        &[],
        inner["controlIdx"].as_u64().unwrap() as usize,
    )
    .unwrap();
    core.paste_internal_in_cell_native(
        0,
        1,
        outer["controlIdx"].as_u64().unwrap() as usize,
        0,
        0,
        0,
    )
    .unwrap();
    let html = core
        .export_control_html_native(0, 1, &[], outer["controlIdx"].as_u64().unwrap() as usize)
        .unwrap();
    // 떠 있는 표만 든 셀 문단은 개체 문단 표시 하나로 감싸 자리 정보를 지킨다(빈 앞 문단은 없다).
    assert_eq!(html.matches("data-rhwp-paragraph=").count(), 1);
    assert!(html.contains(r#"data-rhwp-paragraph="objects""#));
    assert!(!html.contains(r#"data-rhwp-paragraph="inline""#));
    assert_eq!(
        html.matches("<p ").count(),
        1,
        "안쪽 표의 빈 셀만 p로 내보낸다"
    );
}

#[test]
fn marker_tables_and_ordinary_divs_keep_the_existing_recursion_budget() {
    fn contents(paragraphs: &[Paragraph]) -> (usize, String) {
        let mut count = 0;
        let mut text = String::new();
        for para in paragraphs {
            text.push_str(&para.text);
            for control in &para.controls {
                if let Control::Table(table) = control {
                    for cell in &table.cells {
                        let (inner_count, inner_text) = contents(&cell.paragraphs);
                        count += 1 + inner_count;
                        text.push_str(&inner_text);
                    }
                }
            }
        }
        (count, text)
    }
    for tables in [false, true] {
        let mut html = "<span>끝표지</span>".to_string();
        for _ in 0..20 {
            html = if tables {
                format!(
                    r#"<div data-rhwp-paragraph="inline"><table><tr><td>{html}</td></tr></table></div>"#
                )
            } else {
                format!("<div>{html}</div>")
            };
        }
        let mut core = blank();
        core.paste_html_native(0, 0, 0, &html).unwrap();
        let (table_count, text) = contents(&core.document().sections[0].paragraphs);
        assert!(table_count <= 16);
        assert_eq!(text.matches("끝표지").count(), 1);
        if !tables {
            assert_eq!(table_count, 0);
        }
    }
}

#[test]
fn fixed_css_line_height_and_public_cell_format_keep_canonical_double_units() {
    let mut literal = blank();
    literal.paste_html_native(0, 0, 0, r#"<div data-rhwp-paragraph="inline" style="line-height:24px"><span>앞</span><table><tr><td>표</td></tr></table><span>뒤</span></div>"#).unwrap();
    for result in std::iter::once(&literal).chain(saved(&literal).iter()) {
        let para = &result.document().sections[0].paragraphs[0];
        let shape = &result.document().doc_info.para_shapes[para.para_shape_id as usize];
        assert_eq!(shape.line_spacing_type, LineSpacingType::Fixed);
        assert_eq!(
            shape.line_spacing, 3600,
            "24px 고정 줄 높이는 저장 스케일2x HU이다"
        );
        let props: Value =
            serde_json::from_str(&result.get_para_properties_at_native(0, 0).unwrap()).unwrap();
        assert_eq!(props["lineSpacing"], 24.0);
        assert!(result
            .export_selection_html_native(0, 0, 0, 0, 0)
            .unwrap()
            .contains("line-height:24.0px"));
    }
    let mut source = blank();
    let inner: Value = serde_json::from_str(
        &source
            .create_table_ex_native(0, 0, 0, 1, 1, true, Some(&[2003]), Some(&[1207]))
            .unwrap(),
    )
    .unwrap();
    source.insert_paragraph_native(0, 1).unwrap();
    let outer: Value = serde_json::from_str(
        &source
            .create_table_ex_native(0, 1, 0, 1, 1, true, Some(&[20003]), Some(&[8007]))
            .unwrap(),
    )
    .unwrap();
    source
        .copy_control_native(0, 0, &[], inner["controlIdx"].as_u64().unwrap() as usize)
        .unwrap();
    source
        .paste_internal_in_cell_native(
            0,
            1,
            outer["controlIdx"].as_u64().unwrap() as usize,
            0,
            0,
            0,
        )
        .unwrap();
    source
        .apply_para_format_in_cell_native(
            0,
            1,
            outer["controlIdx"].as_u64().unwrap() as usize,
            0,
            0,
            r#"{"alignment":"right","lineSpacingType":"Fixed","lineSpacing":3600}"#,
        )
        .unwrap();
    for source in saved(&source) {
        let outer_index = outer["controlIdx"].as_u64().unwrap() as usize;
        let source_props: Value = serde_json::from_str(
            &source
                .get_cell_para_properties_at_native(0, 1, outer_index, 0, 0)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(source_props["lineSpacing"], 24.0);
        let html = source
            .export_control_html_native(0, 1, &[], outer_index)
            .unwrap();
        assert!(html.contains("line-height:24.0px"));
        let mut target = blank();
        target.paste_html_native(0, 0, 0, &html).unwrap();
        for result in std::iter::once(&target).chain(saved(&target).iter()) {
            let (parent, control, table) = first_table(result);
            let para = &table.cells[0].paragraphs[0];
            let shape = &result.document().doc_info.para_shapes[para.para_shape_id as usize];
            assert_eq!(shape.line_spacing_type, LineSpacingType::Fixed);
            assert_eq!(shape.line_spacing, 3600);
            let props: Value = serde_json::from_str(
                &result
                    .get_cell_para_properties_at_native(0, parent, control, 0, 0)
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(props["lineSpacing"], source_props["lineSpacing"]);
            assert_eq!(props["alignment"], source_props["alignment"]);
        }
    }
}
