//! 저장 줄이 없는 빈 그림 문단도 그림을 한 번 그리고, 예약한 높이를 그대로 쓴다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::renderer::render_tree::{BoundingBox, RenderNode, RenderNodeType};
use serde_json::Value;

fn blank() -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core
}

fn insert_picture(core: &mut DocumentCore, para: usize) {
    let inserted: Value = serde_json::from_str(
        &core
            .insert_picture_native(
                0,
                para,
                0,
                &[],
                include_bytes!("../../assets/logo/logo-16.png"),
                9600,
                9600,
                16,
                16,
                "png",
                "그림 보존",
                None,
                None,
            )
            .unwrap(),
    )
    .unwrap();
    // Session도 삽입 뒤 이 공개 명령으로 글자처럼 배치를 확정한다.
    core.set_picture_properties_native(
        0,
        para,
        inserted["controlIdx"].as_u64().unwrap() as usize,
        r#"{"treatAsChar":true}"#,
    )
    .unwrap();
}

fn nodes(core: &DocumentCore) -> (Vec<BoundingBox>, Vec<BoundingBox>) {
    fn walk(node: &RenderNode, images: &mut Vec<BoundingBox>, tables: &mut Vec<BoundingBox>) {
        match &node.node_type {
            RenderNodeType::Image(_) => images.push(node.bbox),
            RenderNodeType::Table(_) => tables.push(node.bbox),
            _ => {}
        }
        for child in &node.children {
            walk(child, images, tables);
        }
    }
    let (mut images, mut tables) = (Vec::new(), Vec::new());
    for page in 0..core.page_count() {
        walk(
            &core.build_page_render_tree(page).unwrap().root,
            &mut images,
            &mut tables,
        );
        assert_eq!(
            core.render_page_svg_native(page)
                .unwrap()
                .matches("<image ")
                .count(),
            images.len(),
            "이 한 쪽 fixture의 SVG와 실제 이미지 노드 수가 같아야 한다"
        );
    }
    (images, tables)
}

fn assert_bbox(actual: BoundingBox, expected: BoundingBox) {
    for (actual, expected) in [
        (actual.x, expected.x),
        (actual.y, expected.y),
        (actual.width, expected.width),
        (actual.height, expected.height),
    ] {
        assert!(
            (actual - expected).abs() < 0.1,
            "그림 좌표 {actual} != {expected}"
        );
    }
}

fn assert_roundtrip(core: &DocumentCore, expected_tables: usize, no_lineseg_hwpx: bool) {
    assert_eq!(core.page_count(), 1);
    let (images, tables) = nodes(core);
    assert_eq!(images.len(), 1, "저장 전에도 그림은 한 번 그린다");
    assert_eq!(tables.len(), expected_tables);
    assert!((images[0].width - 128.0).abs() < 0.1);
    assert!((images[0].height - 128.0).abs() < 0.1);
    let picture = |core: &DocumentCore| {
        core.document().sections[0]
            .paragraphs
            .iter()
            .find_map(|para| {
                para.controls.iter().find_map(|control| match control {
                    Control::Picture(picture) => Some((para.clone(), picture.clone())),
                    _ => None,
                })
            })
            .unwrap()
    };
    let (_, original) = picture(core);
    for (format, bytes) in [
        ("HWP", core.export_hwp_native().unwrap()),
        ("HWPX", core.export_hwpx_native().unwrap()),
    ] {
        let reopened = DocumentCore::from_bytes(&bytes).unwrap();
        let (para, saved) = picture(&reopened);
        assert_eq!(saved.common.width, original.common.width);
        assert_eq!(saved.common.height, original.common.height);
        assert_eq!(saved.common.treat_as_char, original.common.treat_as_char);
        assert_eq!(
            saved.image_attr.bin_data_id,
            original.image_attr.bin_data_id
        );
        if format == "HWPX" && no_lineseg_hwpx {
            assert!(para.text.is_empty());
            assert!(
                para.line_segs.is_empty(),
                "합성 저장 줄 생략 계약을 유지한다"
            );
        }
        let (saved_images, saved_tables) = nodes(&reopened);
        assert_eq!(
            saved_images.len(),
            1,
            "{format}: 빈 문단의 그림을 한 번 그린다"
        );
        assert_eq!(saved_tables.len(), expected_tables, "{format}: 표 보존");
        assert_eq!(reopened.page_count(), core.page_count());
        assert_bbox(saved_images[0], images[0]);
        for (saved, original) in saved_tables.iter().zip(&tables) {
            assert_bbox(*saved, *original);
        }
    }
}

#[test]
fn portrait_and_landscape_empty_picture_survive_both_formats() {
    for landscape in [false, true] {
        let mut core = blank();
        if landscape {
            core.set_page_def_native(0, r#"{"landscape":true}"#)
                .unwrap();
        }
        insert_picture(&mut core, 0);
        assert_roundtrip(&core, 0, landscape);
    }
}

fn fragment(html: &str) -> &str {
    html.split_once("<!--StartFragment-->\n")
        .unwrap()
        .1
        .split_once("<!--EndFragment-->")
        .unwrap()
        .0
}

#[test]
fn partial_text_table_and_picture_html_copy_survive_both_formats() {
    let mut source = blank();
    source
        .paste_html_native(0, 0, 0, "<p><b>굵은 앞</b></p>")
        .unwrap();
    let created: Value =
        serde_json::from_str(&source.create_table_native(0, 0, 4, 1, 2).unwrap()).unwrap();
    let parent = created["paraIdx"].as_u64().unwrap() as usize;
    let control = created["controlIdx"].as_u64().unwrap() as usize;
    source
        .insert_text_in_cell_native(0, parent, control, 0, 0, 0, "셀 글")
        .unwrap();
    let last = source.document().sections[0].paragraphs.len() - 1;
    insert_picture(&mut source, last);
    let picture_control = source.document().sections[0].paragraphs[last]
        .controls
        .iter()
        .position(|control| matches!(control, Control::Picture(_)))
        .unwrap();
    // 앱의 본문 복사와 같은 공개 경계: 첫 문단 offset 1부터, 표·빈 줄·그림·끝 빈 줄.
    let text = source.export_selection_html_native(0, 0, 1, 0, 4).unwrap();
    let table = source
        .export_control_html_native(0, parent, &[], control)
        .unwrap();
    let image = source
        .export_control_html_native(0, last, &[], picture_control)
        .unwrap();
    let html = format!(
        "{}{}<p></p><p>{}</p><p></p>",
        fragment(&text),
        fragment(&table),
        fragment(&image)
    )
    .replace('\n', "");
    let mut pasted = blank();
    pasted.paste_html_native(0, 0, 0, &html).unwrap();
    assert_eq!(pasted.document().sections[0].paragraphs[0].text, "은 앞");
    let cell_text = |core: &DocumentCore| {
        core.document().sections[0]
            .paragraphs
            .iter()
            .flat_map(|para| &para.controls)
            .find_map(|control| match control {
                Control::Table(table) => Some(table.cells[0].paragraphs[0].text.clone()),
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(cell_text(&pasted), "셀 글");
    assert_roundtrip(&pasted, 1, true);
    for bytes in [
        pasted.export_hwp_native().unwrap(),
        pasted.export_hwpx_native().unwrap(),
    ] {
        assert_eq!(
            cell_text(&DocumentCore::from_bytes(&bytes).unwrap()),
            "셀 글"
        );
    }
}

#[test]
fn nonempty_inline_picture_is_not_emitted_twice() {
    let mut core = blank();
    core.insert_text_native(0, 0, 0, "앞뒤").unwrap();
    core.insert_picture_native(
        0,
        0,
        1,
        &[],
        include_bytes!("../../assets/logo/logo-16.png"),
        9600,
        9600,
        16,
        16,
        "png",
        "글 사이 그림",
        None,
        None,
    )
    .unwrap();
    assert_eq!(core.document().sections[0].paragraphs[0].text, "앞뒤");
    assert_roundtrip(&core, 0, false);
}
