//! 저장 줄이 없는 빈 그림 문단도 그림을 한 번 그리고, 예약한 높이를 그대로 쓴다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::renderer::render_tree::{BoundingBox, RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;
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
        if saved_images.iter().zip(&images).any(|(saved, original)| {
            (saved.x - original.x).abs() >= 0.1 || (saved.y - original.y).abs() >= 0.1
        }) {
            eprintln!(
                "{format}: 원문 문단 {:#?}",
                core.document().sections[0].paragraphs
            );
            eprintln!(
                "{format}: 재열기 문단 {:#?}",
                reopened.document().sections[0].paragraphs
            );
        }
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
    let (table_host, table) = pasted.document().sections[0]
        .paragraphs
        .iter()
        .find_map(|para| match para.controls.as_slice() {
            [Control::Table(table)] => Some((para, table)),
            _ => None,
        })
        .unwrap();
    assert!(
        table_host.line_segs.is_empty(),
        "빈 블록 표에 원문에 없던 글줄을 만들지 않는다"
    );
    let layout: Value =
        serde_json::from_str(&pasted.get_page_text_layout_native(0).unwrap()).unwrap();
    let body_y = |paragraph: usize| {
        layout["runs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|run| run["paraIdx"] == paragraph && run["cellPath"].is_null())
            .unwrap()["y"]
            .as_f64()
            .unwrap()
    };
    let (images, tables) = nodes(&pasted);
    let first_line = &pasted.document().sections[0].paragraphs[0].line_segs[0];
    let empty_line = &pasted.document().sections[0].paragraphs[2].line_segs[0];
    let px = |hu: i32| hu as f64 / 75.0;
    assert!(
        (tables[0].y
            - body_y(0)
            - px(first_line.line_height + first_line.line_spacing)
            - px(i32::from(table.outer_margin_top)))
        .abs()
            < 0.1,
        "표 위 바깥 여백을 한 번 소비한다"
    );
    assert!(
        (body_y(2) - tables[0].y - tables[0].height - px(i32::from(table.outer_margin_bottom)))
            .abs()
            < 0.1,
        "표 아래 바깥 여백 뒤에 실제 빈 문단을 배치한다"
    );
    assert!(
        (images[0].y - body_y(2) - px(empty_line.line_height + empty_line.line_spacing)).abs()
            < 0.1,
        "빈 문단의 한 줄 전진을 그림 앞에서 한 번 소비한다"
    );
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

#[test]
fn preceding_text_edits_preserve_no_lineseg_object_bridge_and_saved_geometry() {
    fn origins(core: &DocumentCore) -> Vec<Vec<i32>> {
        core.document().sections[0]
            .paragraphs
            .iter()
            .map(|para| {
                para.line_segs
                    .iter()
                    .map(|line| line.vertical_pos)
                    .collect()
            })
            .collect()
    }
    fn end(core: &DocumentCore) -> i32 {
        let last = core.document().sections[0].paragraphs[0]
            .line_segs
            .last()
            .unwrap();
        last.vertical_pos + last.line_height.min(last.text_height) + last.line_spacing
    }
    fn saved_svg(core: &DocumentCore) -> Vec<Vec<String>> {
        [
            core.export_hwp_native().unwrap(),
            core.export_hwpx_native().unwrap(),
        ]
        .iter()
        .map(|bytes| {
            let reopened = DocumentCore::from_bytes(bytes).unwrap();
            (0..reopened.page_count())
                .map(|page| reopened.render_page_svg_native(page).unwrap())
                .collect()
        })
        .collect()
    }

    fn assert_delta(core: &DocumentCore, stored: &[Vec<i32>], delta: i32, context: &str) {
        let actual = origins(core);
        assert_eq!(actual.len(), stored.len(), "{context}: 문단 수 보존");
        for (para, (actual, stored)) in actual.iter().zip(stored).enumerate().skip(1) {
            assert_eq!(
                *actual,
                stored.iter().map(|vpos| vpos + delta).collect::<Vec<_>>(),
                "{context}: 문단 {para}의 미수정 개체 점유와 저장 간격"
            );
        }
    }

    let mut producer = blank();
    for (para, text) in ["바깥 본문", "가🦦나다라", "중간", "앞뒤끝말", ""]
        .iter()
        .enumerate()
    {
        if para > 0 {
            producer.insert_paragraph_native(0, para).unwrap();
        }
        producer.insert_text_native(0, para, 0, text).unwrap();
    }
    for (para, offset) in [(1, 1), (3, 1), (3, 3)] {
        let inserted: Value = serde_json::from_str(
            &producer
                .insert_picture_native(
                    0,
                    para,
                    offset,
                    &[],
                    include_bytes!("../../assets/logo/logo-16.png"),
                    1300,
                    1400,
                    16,
                    16,
                    "png",
                    "간격 소유 그림",
                    None,
                    None,
                )
                .unwrap(),
        )
        .unwrap();
        producer
            .set_picture_properties_native(
                0,
                para,
                inserted["controlIdx"].as_u64().unwrap() as usize,
                r#"{"treatAsChar":true}"#,
            )
            .unwrap();
    }
    producer
        .create_shape_control_native(
            0,
            1,
            2,
            1200,
            1400,
            0,
            0,
            true,
            "Square",
            "Rectangle",
            false,
            false,
            &[],
        )
        .unwrap();
    producer
        .apply_char_format_native(0, 1, 0, 2, r#"{"bold":true}"#)
        .unwrap();
    producer
        .apply_char_format_native(0, 3, 2, 4, r#"{"italic":true}"#)
        .unwrap();
    producer.create_table_native(0, 4, 0, 1, 2).unwrap();

    for (format, bytes) in [
        ("HWP", producer.export_hwp_native().unwrap()),
        ("HWPX", producer.export_hwpx_native().unwrap()),
    ] {
        let mut core = HwpDocument::from_bytes(&bytes).unwrap();
        if format == "HWPX" {
            for para in [1, 3] {
                assert!(core.document().sections[0].paragraphs[para]
                    .line_segs
                    .is_empty());
            }
        }
        let original = origins(&core);
        let original_end = end(&core);
        let original_svg = saved_svg(&core);
        let original_saved_origins = [
            core.export_hwp_native().unwrap(),
            core.export_hwpx_native().unwrap(),
        ]
        .iter()
        .map(|bytes| origins(&DocumentCore::from_bytes(bytes).unwrap()))
        .collect::<Vec<_>>();
        let original_shape = core.document().sections[0].paragraphs[0].para_shape_id;
        let original_style = core.document().sections[0].paragraphs[0].style_id;
        for text in ["임시".to_string(), "늘림 ".repeat(80)] {
            core.insert_text_native(0, 0, 0, &text).unwrap();
            let delta = end(&core) - original_end;
            if text.len() > "임시".len() {
                assert!(delta > 0, "{format}: 실제로 여러 줄로 늘어난 편집");
            } else {
                assert_eq!(delta, 0);
            }
            assert_delta(&core, &original, delta, format);
            core.delete_text_native(0, 0, 0, text.chars().count())
                .unwrap();
            assert_eq!(origins(&core), original, "{format}: 역편집 뒤 원본 vpos");
            assert_eq!(
                saved_svg(&core),
                original_svg,
                "{format}: 파일 형식별 재열기 SVG"
            );
        }
        // 비영 간격에서 다시 늘리거나 줄여도 원본 개체 점유를 중복해서 더하지 않는다.
        for (after, expected) in [(1500, 750), (3000, 1500), (1500, 750)] {
            core.apply_para_format_native(0, 0, &format!(r#"{{"spacingAfter":{after}}}"#))
                .unwrap();
            assert_delta(&core, &original, expected, format);
            core.insert_text_native(0, 0, 0, "임시").unwrap();
            core.delete_text_native(0, 0, 0, 2).unwrap();
            assert_delta(&core, &original, expected, format);
            // HWPX는 선택적인 개체 LS를 생략하므로 형식별 원래 저장 계약과 비교한다.
            for ((saved_format, bytes), stored) in [
                ("HWP", core.export_hwp_native().unwrap()),
                ("HWPX", core.export_hwpx_native().unwrap()),
            ]
            .into_iter()
            .zip(&original_saved_origins)
            {
                assert_delta(
                    &DocumentCore::from_bytes(&bytes).unwrap(),
                    stored,
                    expected,
                    &format!("{format} → {saved_format}"),
                );
            }
        }
        let after_shape = core.document().sections[0].paragraphs[0].para_shape_id;
        core.set_para_shape_id_native(0, 0, original_shape).unwrap();
        assert_delta(&core, &original, 0, format);
        let style = core.create_style(&format!(
            r#"{{"name":"개체 앞 간격","type":0,"baseParaShapeId":{after_shape},"baseCharShapeId":0}}"#
        ));
        assert!(style >= 0);
        core.apply_style_native(0, 0, style as usize).unwrap();
        assert_delta(&core, &original, 750, format);
        core.apply_style_native(0, 0, original_style as usize)
            .unwrap();
        assert_delta(&core, &original, 0, format);
        assert_eq!(
            saved_svg(&core),
            original_svg,
            "{format}: 스타일 역적용 뒤 기하"
        );

        // 뒤 문단의 실제 쪽 재시작은 새 문단 간격보다 먼저 보존한다.
        let mut reset = core.document().clone();
        reset.sections[0].paragraphs[2].line_segs[0].vertical_pos = 0;
        reset.sections[0].paragraphs[2].column_type = rhwp::model::paragraph::ColumnBreakType::Page;
        reset.sections[0].raw_stream = None;
        core.set_document(reset);
        let mut reset_origins = origins(&core);
        // 쪽 경계 앞의 개체 문단은 새 간격을 따르고, 재시작 문단부터 원점을 보존한다.
        for vpos in &mut reset_origins[1] {
            *vpos += 750;
        }
        core.apply_para_format_native(0, 0, r#"{"spacingAfter":1500}"#)
            .unwrap();
        assert_eq!(origins(&core), reset_origins, "{format}: 원본 쪽 경계 보존");
    }
}
