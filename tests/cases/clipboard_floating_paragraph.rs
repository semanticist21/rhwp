//! 떠 있는 표·그림의 HTML 복사는 원문 배치·소유 문단·새 식별자를 보존한다.
#![cfg(not(target_arch = "wasm32"))]

use base64::Engine;
use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::model::paragraph::Paragraph;
use rhwp::model::shape::{
    CommonObjAttr, DropCapStyle, HorzAlign, HorzRelTo, ObjectNumberingType, SizeCriterion,
    TextFlow, TextWrap, VertAlign, VertRelTo,
};
use rhwp::model::Padding;
use serde_json::{json, Value};
use std::collections::BTreeSet;

const ATTRIBUTE: &str = "data-rhwp-common-v1";
const TEXT: &str = "가🦦나\n끝";
const PNG: &[u8] = include_bytes!("../../assets/logo/logo-16.png");

fn blank() -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core
}

fn ok(json: &str) -> Value {
    let result: Value = serde_json::from_str(json).unwrap();
    assert_eq!(result["ok"], true, "{result}");
    result
}

fn saved(core: &DocumentCore) -> [DocumentCore; 2] {
    [
        DocumentCore::from_bytes(&core.export_hwp_with_adapter().unwrap()).unwrap(),
        DocumentCore::from_bytes(&core.export_hwpx_native().unwrap()).unwrap(),
    ]
}

fn common(control: &Control) -> Option<&CommonObjAttr> {
    match control {
        Control::Table(table) => Some(&table.common),
        Control::Picture(picture) => Some(&picture.common),
        _ => None,
    }
}

fn common_placement(common: &CommonObjAttr) -> Value {
    let mut value = serde_json::to_value(common).unwrap();
    let object = value.as_object_mut().unwrap();
    object.remove("ctrl_id");
    object.remove("instance_id");
    value
}

fn normalized_common(common: &CommonObjAttr) -> Value {
    let mut value = common_placement(common);
    // 알려진 배치 비트는 아래의 모든 의미 필드로 비교한다. 형식별 저장 고비트는 제외한다.
    value["attr"] = json!(common.attr & 0xe000_0002);
    value
}

fn placement(common: &mut CommonObjAttr, variant: bool, picture: bool) {
    common.attr = if variant { 0x4000_0000 } else { 0x8000_0000 };
    common.treat_as_char = false;
    common.vertical_offset = (-213_i32) as u32;
    common.horizontal_offset = if variant { (-567_i32) as u32 } else { 1234 };
    common.z_order = if variant { -7 } else { 11 };
    common.margin = Padding {
        left: 123,
        right: 456,
        top: 0,
        bottom: -1,
    };
    common.prevent_page_break = i32::from(!variant);
    common.flow_with_text = !variant;
    common.allow_overlap = variant;
    common.affect_line_spacing = true;
    common.hwp5_gen_shape_attr_bit26 = variant;
    common.hwp5_gen_shape_attr_bit28 = !variant;
    common.size_protect = true;
    common.vert_rel_to = if variant {
        VertRelTo::Paper
    } else {
        VertRelTo::Para
    };
    common.vert_align = if variant {
        VertAlign::Outside
    } else {
        VertAlign::Inside
    };
    common.horz_rel_to = if variant {
        HorzRelTo::Page
    } else {
        HorzRelTo::Column
    };
    common.horz_align = if variant {
        HorzAlign::Inside
    } else {
        HorzAlign::Outside
    };
    common.text_wrap = if variant {
        TextWrap::Through
    } else {
        TextWrap::Tight
    };
    common.text_flow = if variant {
        TextFlow::LargestOnly
    } else {
        TextFlow::LeftOnly
    };
    common.width_criterion = if variant {
        SizeCriterion::Paper
    } else {
        SizeCriterion::Absolute
    };
    common.height_criterion = if variant {
        SizeCriterion::Page
    } else {
        SizeCriterion::Absolute
    };
    common.description = format!(
        "떠 있는 {} <&> \"설명\"",
        if picture { "그림" } else { "표" }
    );
    common.numbering_type = if picture {
        ObjectNumberingType::Picture
    } else {
        ObjectNumberingType::Table
    };
    common.drop_cap_style = if variant {
        DropCapStyle::Margin
    } else {
        DropCapStyle::DoubleLine
    };
    common.locked = true;
    common.raw_extra = vec![0x12, 0x34];
}

fn source(variant: bool) -> DocumentCore {
    let mut core = blank();
    core.insert_text_native(0, 0, 0, TEXT).unwrap();
    // 떠 있는 표 생성 API는 별도 문단으로 분할한다. 앵커를 가진 표를 생성한 뒤 배치 IR을 설정한다.
    let table = ok(&core
        .create_table_ex_native(0, 0, 1, 1, 2, true, Some(&[30001, 31002]), Some(&[7007]))
        .unwrap());
    let owner_para = table["paraIdx"].as_u64().unwrap() as usize;
    let table_index = table["controlIdx"].as_u64().unwrap() as usize;
    for (cell, text) in ["첫 셀🦦", "이웃 셀"].iter().enumerate() {
        core.insert_text_in_cell_native(0, owner_para, table_index, cell, 0, 0, text)
            .unwrap();
    }
    core.insert_picture_native(
        0,
        owner_para,
        3,
        &[],
        PNG,
        1701,
        1433,
        16,
        16,
        "png",
        "떠 있는 그림",
        None,
        None,
    )
    .unwrap();
    let mut document = core.document().clone();
    document.sections[0].raw_stream = None;
    for control in &mut document.sections[0].paragraphs[owner_para].controls {
        match control {
            Control::Table(table) => {
                placement(&mut table.common, variant, false);
                table.common.instance_id = 1;
                table.attr = table.common.attr;
                table.outer_margin_left = 12;
                table.outer_margin_right = 34;
                table.outer_margin_top = 56;
                table.outer_margin_bottom = 78;
                table.raw_ctrl_data.clear();
                table.raw_ctrl_seal = None;
            }
            Control::Picture(picture) => {
                placement(&mut picture.common, variant, true);
                picture.common.instance_id = 3;
                picture.instance_id = 4;
            }
            _ => {}
        }
    }
    core.set_document(document);
    assert_eq!(owner_para, 0, "같은 원문 문단에 개체를 삽입한다");
    assert_eq!(core.document().sections[0].paragraphs.len(), 1);
    let paragraph = &core.document().sections[0].paragraphs[owner_para];
    assert_eq!(paragraph.text, TEXT);
    let offsets: Vec<_> = paragraph
        .controls
        .iter()
        .zip(paragraph.control_text_positions())
        .filter(|(control, _)| common(control).is_some())
        .map(|(_, at)| at)
        .collect();
    assert_eq!(offsets, [1, 3], "공개 삽입에 넘긴 원문 scalar 자리");
    core
}

fn object_contents(
    core: &DocumentCore,
    root_para: usize,
    paragraph: &Paragraph,
    path: &[(usize, usize, usize)],
) -> Value {
    let positions = paragraph.control_text_positions();
    let objects: Vec<_> = paragraph.controls.iter().enumerate().filter_map(|(index, control)| {
        let common = common(control)?;
        let mut object = json!({"at":positions[index], "common":normalized_common(common)});
        match control {
            Control::Table(table) => {
                object["kind"] = json!("table");
                object["rows"] = json!(table.row_count);
                object["cols"] = json!(table.col_count);
                object["outer_margin"] = json!([table.outer_margin_left, table.outer_margin_right, table.outer_margin_top, table.outer_margin_bottom]);
                object["padding"] = json!(table.padding);
                object["cell_spacing"] = json!(table.cell_spacing);
                object["cells"] = json!(table.cells.iter().enumerate().map(|(cell_index, cell)| {
                    json!({
                        "row":cell.row,"col":cell.col,"row_span":cell.row_span,"col_span":cell.col_span,
                        "width":cell.width,"height":cell.height,
                        "padding":cell.padding,"apply_inner_margin":cell.apply_inner_margin,
                        "paragraphs":cell.paragraphs.iter().enumerate().map(|(para_index, para)| {
                            let mut inner_path = path.to_vec();
                            inner_path.push((index, cell_index, para_index));
                            object_contents(core, root_para, para, &inner_path)
                        }).collect::<Vec<_>>()
                    })
                }).collect::<Vec<_>>());
            }
            Control::Picture(_) => {
                object["kind"] = json!("picture");
                let data = core.get_control_image_data_native(0, root_para, path, index).unwrap();
                assert_eq!(data, PNG, "원래 임베드 그림 바이트");
                object["data"] = json!(base64::engine::general_purpose::STANDARD.encode(data));
            }
            _ => unreachable!(),
        }
        Some(object)
    }).collect();
    json!({"text":paragraph.text,"objects":objects})
}

fn body_contents(core: &DocumentCore) -> Value {
    json!(core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .map(|(index, para)| object_contents(core, index, para, &[]))
        .collect::<Vec<_>>())
}

fn metadata_values(html: &str) -> Vec<Value> {
    let needle = format!("{ATTRIBUTE}=\"");
    html.split(&needle)
        .skip(1)
        .map(|part| {
            let encoded = part.split('"').next().unwrap();
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .unwrap();
            let common: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(common["ctrl_id"], 0);
            assert_eq!(common["instance_id"], 0);
            assert_eq!(common["treat_as_char"], false);
            common
        })
        .collect()
}

fn first_control(core: &DocumentCore, picture: bool) -> (usize, usize) {
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
                    (if picture {
                        matches!(control, Control::Picture(_))
                    } else {
                        matches!(control, Control::Table(_))
                    })
                    .then_some((para, index))
                })
        })
        .unwrap()
}

fn copied_control(core: &DocumentCore, picture: bool) -> String {
    let (para, index) = first_control(core, picture);
    let before = format!("{:?}", core.document());
    let events = core.serialize_event_log();
    let bytes = [
        core.export_hwp_with_adapter().unwrap(),
        core.export_hwpx_native().unwrap(),
    ];
    let html = core
        .export_control_html_native(0, para, &[], index)
        .unwrap();
    assert_eq!(
        format!("{:?}", core.document()),
        before,
        "읽기 전용 HTML은 원문을 바꾸지 않는다"
    );
    assert_eq!(core.serialize_event_log(), events);
    assert_eq!(
        [
            core.export_hwp_with_adapter().unwrap(),
            core.export_hwpx_native().unwrap()
        ],
        bytes
    );
    let floating = !common(&core.document().sections[0].paragraphs[para].controls[index])
        .unwrap()
        .treat_as_char;
    assert_eq!(metadata_values(&html).len(), usize::from(floating));
    html
}

fn fragment(html: &str) -> &str {
    html.split_once("<!--StartFragment-->")
        .unwrap()
        .1
        .split_once("<!--EndFragment-->")
        .unwrap()
        .0
        .trim()
}

fn float_fragment(core: &DocumentCore) -> String {
    let table = copied_control(core, false);
    let picture = copied_control(core, true);
    format!("<div data-rhwp-paragraph=\"objects\"><span>가</span>{}<span>🦦나</span>{}<br><span>끝</span></div>", fragment(&table), fragment(&picture))
}

fn all_ids(paragraphs: &[Paragraph], output: &mut Vec<u32>) {
    for paragraph in paragraphs {
        for control in &paragraph.controls {
            if let Some(common) = common(control) {
                output.push(common.instance_id);
            }
            if let Control::Picture(picture) = control {
                output.push(picture.instance_id);
            }
            if let Control::Table(table) = control {
                if !table.raw_ctrl_data.is_empty() {
                    let raw_id =
                        u32::from_le_bytes(table.raw_ctrl_data[32..36].try_into().unwrap());
                    assert_eq!(
                        raw_id, table.common.instance_id,
                        "저장용 표 헤더의 ID도 새 번호와 일치한다"
                    );
                }
                for cell in &table.cells {
                    all_ids(&cell.paragraphs, output);
                }
            }
        }
    }
}

fn assert_unique_ids(core: &DocumentCore) {
    let mut ids = Vec::new();
    for section in &core.document().sections {
        all_ids(&section.paragraphs, &mut ids);
    }
    assert!(!ids.is_empty());
    assert!(
        ids.iter().all(|id| *id != 0),
        "파싱한 개체에는 새 ID가 필요하다: {ids:?}"
    );
    assert_eq!(
        ids.iter().copied().collect::<BTreeSet<_>>().len(),
        ids.len(),
        "기존·새·중첩 개체 ID를 공유하지 않는다: {ids:?}"
    );
}

#[test]
fn floating_controls_export_all_common_placement_and_reopen_each_source_format() {
    for variant in [false, true] {
        let original = source(variant);
        let reopened = saved(&original);
        for source in std::iter::once(original).chain(reopened) {
            for picture in [false, true] {
                let (para, index) = first_control(&source, picture);
                let expected = common_placement(
                    common(&source.document().sections[0].paragraphs[para].controls[index])
                        .unwrap(),
                );
                let html = copied_control(&source, picture);
                let mut target = blank();
                ok(&target.paste_html_native(0, 0, 0, &html).unwrap());
                let (target_para, target_index) = first_control(&target, picture);
                assert_eq!(
                    common_placement(
                        common(
                            &target.document().sections[0].paragraphs[target_para].controls
                                [target_index]
                        )
                        .unwrap()
                    ),
                    expected
                );
                if picture {
                    assert_eq!(
                        target
                            .get_control_image_data_native(0, target_para, &[], target_index)
                            .unwrap(),
                        PNG
                    );
                }
                // 저장 형식 자체의 원문 변환은 대조군에도 적용한다. 기존 포맷 손실을 이번 복사 탓으로 만들지 않는다.
                for (source_saved, target_saved) in saved(&source).into_iter().zip(saved(&target)) {
                    let (s_para, s_index) = first_control(&source_saved, picture);
                    let (t_para, t_index) = first_control(&target_saved, picture);
                    assert_eq!(
                        normalized_common(
                            common(
                                &target_saved.document().sections[0].paragraphs[t_para].controls
                                    [t_index]
                            )
                            .unwrap()
                        ),
                        normalized_common(
                            common(
                                &source_saved.document().sections[0].paragraphs[s_para].controls
                                    [s_index]
                            )
                            .unwrap()
                        )
                    );
                }
            }
        }
    }
}

#[test]
fn grouped_float_table_and_picture_keep_one_paragraph_anchors_cells_and_source() {
    let original = source(false);
    let reopened = saved(&original);
    for (source_format, source) in std::iter::once(original).chain(reopened).enumerate() {
        let before = format!("{:?}", source.document());
        let events = source.serialize_event_log();
        let expected = body_contents(&source);
        let html = float_fragment(&source);
        assert_eq!(metadata_values(&html).len(), 2);
        assert_eq!(format!("{:?}", source.document()), before);
        assert_eq!(source.serialize_event_log(), events);
        let mut target = blank();
        ok(&target.paste_html_native(0, 0, 0, &html).unwrap());
        assert_eq!(target.document().sections[0].paragraphs.len(), 1);
        assert_eq!(body_contents(&target), expected);
        for (format, (source_saved, target_saved)) in
            saved(&source).into_iter().zip(saved(&target)).enumerate()
        {
            assert!(!source_saved.document().bin_data_content.is_empty(), "복사 전 원문 그림: source_format {source_format}, saved_format {format}, source bins {:?}, doc_info {:?}", source.document().bin_data_content.iter().map(|bin| (bin.id, bin.extension.as_str())).collect::<Vec<_>>(), source.document().doc_info.bin_data_list);
            assert_eq!(body_contents(&target_saved), body_contents(&source_saved));
        }
    }
}

#[test]
fn objects_marker_interleaves_one_floating_and_one_inline_control() {
    for inline_table in [false, true] {
        let mut source = source(false);
        let mut document = source.document().clone();
        for control in &mut document.sections[0].paragraphs[0].controls {
            match control {
                Control::Table(table) => {
                    table.common.treat_as_char = inline_table;
                    table.attr = (table.attr & !1) | u32::from(inline_table);
                }
                Control::Picture(picture) => picture.common.treat_as_char = !inline_table,
                _ => {}
            }
        }
        source.set_document(document);
        let html = float_fragment(&source);
        assert_eq!(
            metadata_values(&html).len(),
            1,
            "떠 있는 개체만 배치 메타데이터를 내보낸다"
        );
        let mut target = blank();
        target.paste_html_native(0, 0, 0, &html).unwrap();
        let source_control = first_control(&source, inline_table);
        let target_control = first_control(&target, inline_table);
        assert_eq!(
            normalized_common(
                common(
                    &target.document().sections[0].paragraphs[target_control.0].controls
                        [target_control.1]
                )
                .unwrap()
            ),
            normalized_common(
                common(
                    &source.document().sections[0].paragraphs[source_control.0].controls
                        [source_control.1]
                )
                .unwrap()
            )
        );
        let reopened = saved(&target);
        for result in std::iter::once(target).chain(reopened) {
            let paragraph = &result.document().sections[0].paragraphs[0];
            assert_eq!(result.document().sections[0].paragraphs.len(), 1);
            assert_eq!(paragraph.text, TEXT);
            let objects: Vec<_> = paragraph
                .controls
                .iter()
                .zip(paragraph.control_text_positions())
                .filter(|(control, _)| common(control).is_some())
                .collect();
            assert_eq!(
                objects.iter().map(|(_, at)| *at).collect::<Vec<_>>(),
                [1, 3]
            );
            let (table_para, table_index) = first_control(&result, false);
            let (picture_para, picture_index) = first_control(&result, true);
            assert_eq!((table_para, picture_para), (0, 0));
            let Control::Table(table) = &paragraph.controls[table_index] else {
                panic!("첫 개체는 표");
            };
            let Control::Picture(picture) = &paragraph.controls[picture_index] else {
                panic!("둘째 개체는 그림");
            };
            assert_eq!(table.common.treat_as_char, inline_table);
            assert_eq!(picture.common.treat_as_char, !inline_table);
            assert_eq!(
                table
                    .cells
                    .iter()
                    .map(|cell| cell.paragraphs[0].text.as_str())
                    .collect::<Vec<_>>(),
                ["첫 셀🦦", "이웃 셀"]
            );
            assert_eq!(
                result
                    .get_control_image_data_native(0, picture_para, &[], picture_index)
                    .unwrap(),
                PNG
            );
            assert_unique_ids(&result);
        }
    }
}

#[test]
fn float_table_width_and_cells_survive_while_unmarked_and_inline_tables_stay_bounded() {
    for dimensions in [(65009, 9901), (30000, 9901), (0, 0)] {
        let mut source = source(false);
        let (owner_para, table_index) = first_control(&source, false);
        let mut document = source.document().clone();
        let Control::Table(table) =
            &mut document.sections[0].paragraphs[owner_para].controls[table_index]
        else {
            unreachable!()
        };
        // 개체 크기와 셀의 합은 별개 원문 값이다. 유효한 0 sentinel도 셀 폭으로 덮지 않는다.
        (table.common.width, table.common.height) = dimensions;
        source.set_document(document);
        let html = copied_control(&source, false);
        let (para, index) = first_control(&source, false);
        let Control::Table(table) = &source.document().sections[0].paragraphs[para].controls[index]
        else {
            unreachable!()
        };
        assert_eq!(
            table
                .cells
                .iter()
                .map(|cell| cell.width)
                .collect::<Vec<_>>(),
            [30001, 31002]
        );
        let mut target = blank();
        target.paste_html_native(0, 0, 0, &html).unwrap();
        let (para, index) = first_control(&target, false);
        let Control::Table(actual) =
            &target.document().sections[0].paragraphs[para].controls[index]
        else {
            unreachable!()
        };
        assert_eq!((actual.common.width, actual.common.height), dimensions);
        assert_eq!(
            actual
                .cells
                .iter()
                .map(|cell| (cell.width, cell.height))
                .collect::<Vec<_>>(),
            [(30001, 7007), (31002, 7007)]
        );
        for (source_saved, target_saved) in saved(&source).into_iter().zip(saved(&target)) {
            let (s_para, s_index) = first_control(&source_saved, false);
            let (t_para, t_index) = first_control(&target_saved, false);
            assert_eq!(
                normalized_common(
                    common(
                        &target_saved.document().sections[0].paragraphs[t_para].controls[t_index]
                    )
                    .unwrap()
                ),
                normalized_common(
                    common(
                        &source_saved.document().sections[0].paragraphs[s_para].controls[s_index]
                    )
                    .unwrap()
                )
            );
        }
    }
    for (marker, inline) in [
        ("", false),
        (" data-rhwp-geometry=\"exact\"", false),
        (" data-rhwp-geometry=\"exact\"", true),
    ] {
        let mut ordinary = blank();
        let table_html = format!("<table{marker} style=\"width:610.03pt\"><tr><td style=\"width:300.01pt\">첫 셀</td><td style=\"width:310.02pt\">이웃 셀</td></tr></table>");
        let html = if inline {
            format!("<div data-rhwp-paragraph=\"inline\">{table_html}</div>")
        } else {
            table_html
        };
        ordinary.paste_html_native(0, 0, 0, &html).unwrap();
        let (para, index) = first_control(&ordinary, false);
        let Control::Table(table) =
            &ordinary.document().sections[0].paragraphs[para].controls[index]
        else {
            unreachable!()
        };
        assert!(table.common.treat_as_char);
        assert!(table.common.width < 61003);
        assert_eq!(
            table.cells.iter().map(|cell| cell.width).sum::<u32>(),
            table.common.width
        );
    }
}

fn replace_metadata(html: &str, value: &Value) -> String {
    let encoded =
        base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(value).unwrap());
    replace_encoded_metadata(html, &encoded)
}

fn replace_encoded_metadata(html: &str, encoded: &str) -> String {
    let needle = format!("{ATTRIBUTE}=\"");
    let start = html.find(&needle).unwrap() + needle.len();
    let end = start + html[start..].find('"').unwrap();
    format!("{}{}{}", &html[..start], encoded, &html[end..])
}

#[test]
fn invalid_float_metadata_with_prior_font_and_image_fails_atomically() {
    let source = source(false);
    let exported = copied_control(&source, true);
    let html = fragment(&exported);
    let valid = metadata_values(html).into_iter().next().unwrap();
    let mut invalid = vec![
        replace_encoded_metadata(html, "not-base64!"),
        replace_encoded_metadata(
            html,
            &base64::engine::general_purpose::STANDARD.encode([0xff, 0xfe]),
        ),
        replace_encoded_metadata(
            html,
            &base64::engine::general_purpose::STANDARD.encode(b"null"),
        ),
    ];
    let src_start = html.find("src=\"").unwrap();
    let src_end = src_start + 5 + html[src_start + 5..].find('"').unwrap();
    for src in [
        "data:image/png;base64,not-base64!",
        "",
        "https://example.com/floating.png",
    ] {
        invalid.push(format!(
            "{}src=\"{src}\"{}",
            &html[..src_start],
            &html[src_end + 1..]
        ));
    }
    invalid.push(format!("{}{}", &html[..src_start], &html[src_end + 1..]));
    for spelling in [
        format!("{ATTRIBUTE} = "),
        format!("{ATTRIBUTE}= "),
        format!("{ATTRIBUTE} ="),
    ] {
        invalid.push(replace_encoded_metadata(html, "not-base64!").replacen(
            &format!("{ATTRIBUTE}="),
            &spelling,
            1,
        ));
    }
    invalid.push(html.replacen(ATTRIBUTE, "data-rhwp-common-v2", 1));
    let common_start = html.find(&format!("{ATTRIBUTE}=\"")).unwrap() + ATTRIBUTE.len() + 2;
    let common_end = common_start + html[common_start..].find('"').unwrap();
    invalid.push(format!(
        "<div data-rhwp-paragraph=\"objects\">앞{}{}뒤</div>",
        &html[..common_end],
        &html[common_end + 1..]
    ));
    let encoded =
        base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&valid).unwrap());
    invalid.push(html.replacen(
        &format!("{ATTRIBUTE}=\""),
        &format!("{ATTRIBUTE}=\"{encoded}\" {ATTRIBUTE}=\""),
        1,
    ));
    let exported_table = copied_control(&source, false);
    let table_common = metadata_values(&exported_table).into_iter().next().unwrap();
    let table_encoded = base64::engine::general_purpose::STANDARD
        .encode(serde_json::to_vec(&table_common).unwrap());
    invalid.push(format!("<table {ATTRIBUTE}=\"{table_encoded}\"></table>"));
    for style in [
        "width:42949672.95pt;height:70.07pt",
        "width:70.07pt;height:42949672.95pt",
    ] {
        invalid.push(format!("<table {ATTRIBUTE}=\"{table_encoded}\" data-rhwp-geometry=\"exact\"><tr><td style=\"{style}\">원문</td></tr></table>"));
    }
    for (key, value) in [
        ("width", json!(2147483648_u64)),
        ("height", json!(2147483648_u64)),
        ("width", json!("1701")),
        ("instance_id", json!(1)),
        ("ctrl_id", json!(1)),
        ("treat_as_char", json!("false")),
        ("treat_as_char", json!(true)),
        ("vert_rel_to", json!("Unknown")),
        ("text_wrap", json!("Unknown")),
        ("z_order", json!(2147483648_u64)),
        ("margin", json!({"left":40000,"right":0,"top":0,"bottom":0})),
        ("raw_extra", json!([256])),
        ("height_criterion", json!("Column")),
        ("unknown_field", json!(true)),
    ] {
        let mut value_json = valid.clone();
        value_json[key] = value;
        invalid.push(replace_metadata(html, &value_json));
    }
    let mut missing = valid.clone();
    missing.as_object_mut().unwrap().remove("horizontal_offset");
    invalid.push(replace_metadata(html, &missing));
    let mut oversized_description = valid.clone();
    oversized_description["description"] = json!("🦦".repeat(32768));
    invalid.push(replace_metadata(html, &oversized_description));
    let image = base64::engine::general_purpose::STANDARD.encode(PNG);
    for invalid in invalid {
        let mut target = blank();
        target.insert_text_native(0, 0, 0, "보존할 원문🦦").unwrap();
        let before = format!("{:?}", target.document());
        let doc_info = format!("{:?}", target.document().doc_info);
        let bins = target.document().bin_data_content.len();
        let events = target.serialize_event_log();
        let bytes = [
            target.export_hwp_with_adapter().unwrap(),
            target.export_hwpx_native().unwrap(),
        ];
        let html = format!("<p><b>먼저 읽은 글</b><img src=\"data:image/png;base64,{image}\" width=\"16\" height=\"16\"></p>{invalid}");
        assert!(
            target.paste_html_native(0, 0, 0, &html).is_err(),
            "잘못된 개체 배치는 부분 적용하지 않는다"
        );
        assert_eq!(format!("{:?}", target.document()), before);
        assert_eq!(format!("{:?}", target.document().doc_info), doc_info);
        assert_eq!(target.document().bin_data_content.len(), bins);
        assert_eq!(target.serialize_event_log(), events);
        assert_eq!(
            [
                target.export_hwp_with_adapter().unwrap(),
                target.export_hwpx_native().unwrap()
            ],
            bytes
        );
    }
}

#[test]
fn parsed_float_ids_are_fresh_across_existing_siblings_and_child_contexts() {
    let origin = source(false);
    let html = float_fragment(&origin);
    for nested in [false, true] {
        let mut target = source(false);
        let mut before_ids = Vec::new();
        all_ids(&target.document().sections[0].paragraphs, &mut before_ids);
        target.insert_paragraph_native(0, 1).unwrap();
        let fragment = if nested {
            format!("<table><tr><td>{html}</td><td>{html}</td></tr></table>")
        } else {
            format!("{html}{html}")
        };
        ok(&target.paste_html_native(0, 1, 0, &fragment).unwrap());
        assert_unique_ids(&target);
        let copied_contents = |result: &DocumentCore, root_para: usize| -> Vec<Value> {
            if nested {
                let controls = &result.document().sections[0].paragraphs[root_para].controls;
                let outer_index = controls
                    .iter()
                    .position(|control| matches!(control, Control::Table(_)))
                    .unwrap();
                let Control::Table(outer) = &controls[outer_index] else {
                    panic!("바깥 표");
                };
                assert_eq!(outer.cells.len(), 2);
                outer
                    .cells
                    .iter()
                    .enumerate()
                    .map(|(cell, value)| {
                        assert_eq!(value.paragraphs.len(), 1);
                        object_contents(
                            result,
                            root_para,
                            &value.paragraphs[0],
                            &[(outer_index, cell, 0)],
                        )
                    })
                    .collect()
            } else {
                result.document().sections[0].paragraphs[root_para..root_para + 2]
                    .iter()
                    .enumerate()
                    .map(|(index, para)| object_contents(result, root_para + index, para, &[]))
                    .collect()
            }
        };
        let expected = body_contents(&origin)[0].clone();
        assert_eq!(
            copied_contents(&target, 1),
            [expected.clone(), expected.clone()]
        );
        let mut after_ids = Vec::new();
        all_ids(&target.document().sections[0].paragraphs, &mut after_ids);
        assert!(
            before_ids.iter().all(|id| after_ids.contains(id)),
            "기존 ID를 바꾸지 않는다"
        );
        for (source_saved, reopened) in saved(&origin).into_iter().zip(saved(&target)) {
            assert_unique_ids(&reopened);
            let expected = body_contents(&source_saved)[0].clone();
            assert_eq!(copied_contents(&reopened, 1), [expected.clone(), expected]);
        }
        if nested {
            let before = format!("{:?}", target.document());
            let events = target.serialize_event_log();
            let outer_index = target.document().sections[0].paragraphs[1]
                .controls
                .iter()
                .position(|control| matches!(control, Control::Table(_)))
                .unwrap();
            let html = target
                .export_control_html_native(0, 1, &[], outer_index)
                .unwrap();
            assert_eq!(html.matches("data-rhwp-paragraph=\"objects\"").count(), 2);
            assert_eq!(metadata_values(&html).len(), 4);
            assert_eq!(format!("{:?}", target.document()), before);
            assert_eq!(target.serialize_event_log(), events);
            let mut recopied = blank();
            recopied.paste_html_native(0, 0, 0, &html).unwrap();
            assert_eq!(copied_contents(&recopied, 0), [expected.clone(), expected]);
            assert_unique_ids(&recopied);
            for (source_saved, reopened) in saved(&origin).into_iter().zip(saved(&recopied)) {
                let expected = body_contents(&source_saved)[0].clone();
                assert_eq!(copied_contents(&reopened, 0), [expected.clone(), expected]);
                assert_unique_ids(&reopened);
            }
        }
    }
}

#[test]
fn objects_marker_preserves_empty_and_text_prefix_suffix_paragraphs() {
    let source = source(false);
    let picture = copied_control(&source, true);
    let picture = fragment(&picture);
    for text in ["", "앞🦦뒤"] {
        let html = format!("<p></p><div data-rhwp-paragraph=\"objects\"><span>{text}</span>{picture}</div><p></p><p>끝<br>줄</p>");
        let mut target = blank();
        target.paste_html_native(0, 0, 0, &html).unwrap();
        let saved_target = saved(&target);
        for reopened in std::iter::once(target).chain(saved_target) {
            let paragraphs = &reopened.document().sections[0].paragraphs;
            assert_eq!(
                paragraphs
                    .iter()
                    .map(|para| para.text.as_str())
                    .collect::<Vec<_>>(),
                ["", text, "", "끝\n줄"]
            );
            assert_eq!(paragraphs[1].controls.len(), 1);
            assert!(!common(&paragraphs[1].controls[0]).unwrap().treat_as_char);
            assert_eq!(
                paragraphs[1].control_text_positions(),
                [text.chars().count()]
            );
        }
    }
}

#[test]
fn float_metadata_accepts_attribute_spaces_and_ignores_names_in_quoted_values() {
    let source = source(false);
    let exported = copied_control(&source, true);
    let html = fragment(&exported);
    let (para, index) = first_control(&source, true);
    let expected = common_placement(
        common(&source.document().sections[0].paragraphs[para].controls[index]).unwrap(),
    );
    for spelling in [
        format!("{ATTRIBUTE} = "),
        format!("{ATTRIBUTE}= "),
        format!("{ATTRIBUTE} ="),
    ] {
        let mut target = blank();
        target
            .paste_html_native(
                0,
                0,
                0,
                &html.replacen(&format!("{ATTRIBUTE}="), &spelling, 1),
            )
            .unwrap();
        let (para, index) = first_control(&target, true);
        assert_eq!(
            common_placement(
                common(&target.document().sections[0].paragraphs[para].controls[index]).unwrap()
            ),
            expected
        );
    }
    let image = base64::engine::general_purpose::STANDARD.encode(PNG);
    let html = format!("<img title=\"data-rhwp-common-v2 invalid\" src=\"data:image/png;base64,{image}\" width=\"16\" height=\"16\">");
    let mut ordinary = blank();
    ordinary.paste_html_native(0, 0, 0, &html).unwrap();
    let (para, index) = first_control(&ordinary, true);
    assert!(
        common(&ordinary.document().sections[0].paragraphs[para].controls[index])
            .unwrap()
            .treat_as_char
    );
}

#[test]
fn float_metadata_counts_only_the_selected_cf_html_fragment() {
    let source = source(false);
    let exported = copied_control(&source, true);
    let image = fragment(&exported);
    let (para, index) = first_control(&source, true);
    let expected = common_placement(
        common(&source.document().sections[0].paragraphs[para].controls[index]).unwrap(),
    );
    for context in [
        image.to_owned(),
        replace_encoded_metadata(image, "not-base64!"),
    ] {
        let html = format!("{context}<!--StartFragment--><div data-rhwp-paragraph=\"objects\">{image}</div><!--EndFragment-->{context}");
        let mut target = blank();
        target.paste_html_native(0, 0, 0, &html).unwrap();
        let objects: Vec<_> = target.document().sections[0]
            .paragraphs
            .iter()
            .flat_map(|para| &para.controls)
            .filter_map(common)
            .collect();
        assert_eq!(objects.len(), 1);
        assert_eq!(common_placement(objects[0]), expected);
    }
}

#[test]
fn inline_companion_default_placement_stays_the_same_in_both_saved_formats() {
    let mut target = blank();
    target.paste_html_native(0,0,0,"<div data-rhwp-paragraph=\"objects\">가<table style=\"width:30.01pt;height:25.03pt\" data-rhwp-geometry=\"exact\"><tr><td style=\"width:30.01pt;height:25.03pt\">안쪽표</td></tr></table>나</div>").unwrap();
    for reopened in std::iter::once(target.export_hwpx_native().unwrap())
        .chain([target.export_hwp_with_adapter().unwrap()])
    {
        let result = DocumentCore::from_bytes(&reopened).unwrap();
        let (para, index) = first_control(&result, false);
        let value =
            common(&result.document().sections[0].paragraphs[para].controls[index]).unwrap();
        assert!(value.treat_as_char);
        assert_eq!(value.vert_rel_to, VertRelTo::Page);
        assert!(!value.flow_with_text);
        assert_eq!(value.horz_rel_to, HorzRelTo::Para);
        assert_eq!(value.text_wrap, TextWrap::TopAndBottom);
        assert_eq!((value.width, value.height), (3001, 2503));
        assert_eq!(result.document().sections[0].paragraphs[para].text, "가나");
        assert_eq!(
            result.document().sections[0].paragraphs[para]
                .control_text_positions()
                .last(),
            Some(&1)
        );
    }
}
