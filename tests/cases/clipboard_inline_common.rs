//! 인라인 Common v2는 원문 배치를 보존하고, 표에는 기존 본문 폭 제한을 적용한다.
#![cfg(not(target_arch = "wasm32"))]

use base64::Engine;
use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::model::paragraph::Paragraph;
use rhwp::model::shape::{
    CommonObjAttr, DropCapStyle, HorzAlign, HorzRelTo, ObjectNumberingType, TextFlow, TextWrap,
    VertAlign, VertRelTo,
};
use rhwp::model::Padding;
use serde_json::{json, Value};
use std::collections::BTreeSet;

const V1: &str = "data-rhwp-common-v1";
const V2: &str = "data-rhwp-common-v2";
const TEXT: &str = "가🦦나\n끝";
const PNG: &[u8] = include_bytes!("../../assets/logo/logo-16.png");

fn blank() -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core
}

fn ok(text: &str) -> Value {
    let value: Value = serde_json::from_str(text).unwrap();
    assert_eq!(value["ok"], true, "{value}");
    value
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

fn source(variant: bool, wide: bool, inline_table: bool, inline_picture: bool) -> DocumentCore {
    let mut core = blank();
    core.insert_text_native(0, 0, 0, TEXT).unwrap();
    let columns = if wide { [25001, 35002] } else { [3001, 6002] };
    let table = ok(&core
        .create_table_ex_native(0, 0, 1, 1, 2, true, Some(&columns), Some(&[7007]))
        .unwrap());
    let control = table["controlIdx"].as_u64().unwrap() as usize;
    for (cell, text) in ["첫 셀🦦", "이웃 셀"].iter().enumerate() {
        core.insert_text_in_cell_native(0, 0, control, cell, 0, 0, text)
            .unwrap();
    }
    core.insert_picture_native(
        0,
        0,
        3,
        &[],
        PNG,
        1701,
        1433,
        16,
        16,
        "png",
        "인라인 그림",
        None,
        None,
    )
    .unwrap();
    let mut document = core.document().clone();
    document.sections[0].raw_stream = None;
    for control in &mut document.sections[0].paragraphs[0].controls {
        let (c, picture) = match control {
            Control::Table(table) => {
                table.raw_ctrl_data.clear();
                table.raw_ctrl_seal = None;
                table.common.margin = Padding {
                    left: 283,
                    right: 283,
                    top: 283,
                    bottom: 283,
                };
                table.outer_margin_left = 283;
                table.outer_margin_right = 283;
                table.outer_margin_top = 283;
                table.outer_margin_bottom = 283;
                (&mut table.common, false)
            }
            Control::Picture(picture) => {
                picture.instance_id = 25;
                (&mut picture.common, true)
            }
            _ => continue,
        };
        c.attr = if variant { 0x4000_0000 } else { 0x8000_0000 };
        c.treat_as_char = if picture {
            inline_picture
        } else {
            inline_table
        };
        c.attr |= u32::from(c.treat_as_char);
        c.instance_id = if picture { 23 } else { 17 };
        c.vertical_offset = if variant { (-213_i32) as u32 } else { 1303 };
        c.horizontal_offset = if variant { (-567_i32) as u32 } else { 1201 };
        c.z_order = if variant { -7 } else { 11 };
        c.prevent_page_break = 1;
        c.flow_with_text = false;
        c.allow_overlap = true;
        c.affect_line_spacing = true;
        c.size_protect = true;
        c.vert_rel_to = if variant {
            VertRelTo::Paper
        } else {
            VertRelTo::Para
        };
        c.horz_rel_to = if variant {
            HorzRelTo::Column
        } else {
            HorzRelTo::Page
        };
        c.vert_align = if variant {
            VertAlign::Outside
        } else {
            VertAlign::Bottom
        };
        c.horz_align = if variant {
            HorzAlign::Inside
        } else {
            HorzAlign::Right
        };
        c.text_wrap = if variant {
            TextWrap::BehindText
        } else {
            TextWrap::InFrontOfText
        };
        c.text_flow = if variant {
            TextFlow::LargestOnly
        } else {
            TextFlow::LeftOnly
        };
        c.numbering_type = if picture {
            ObjectNumberingType::Picture
        } else {
            ObjectNumberingType::Table
        };
        c.drop_cap_style = DropCapStyle::Margin;
        c.description = format!(
            "인라인 {} <&> \"설명🦦\"",
            if picture { "그림" } else { "표" }
        );
        c.locked = true;
        c.raw_extra = vec![0x12, 0x34];
        if let Control::Table(table) = control {
            table.attr = table.common.attr;
        }
    }
    core.set_document(document);
    // 개체 삽입이 저장 글자 경계를 옮기므로 모든 배치를 마친 뒤 원문 서식을 적용한다.
    core.apply_char_format_native(0, 0, 1, 3, r##"{"bold":true,"textColor":"#cc0000"}"##)
        .unwrap();
    assert_source(&core, variant, wide, inline_table, inline_picture, 283);
    core
}

fn objects(paragraph: &Paragraph) -> Vec<(usize, &Control)> {
    paragraph
        .controls
        .iter()
        .enumerate()
        .filter(|(_, c)| common(c).is_some())
        .collect()
}

fn assert_source(
    core: &DocumentCore,
    variant: bool,
    wide: bool,
    inline_table: bool,
    inline_picture: bool,
    outer_lr: i16,
) {
    let paragraph = &core.document().sections[0].paragraphs[0];
    assert_eq!(paragraph.text, TEXT);
    let positions = paragraph.control_text_positions();
    let controls = objects(paragraph);
    assert_eq!(controls.len(), 2);
    assert_eq!(
        controls
            .iter()
            .map(|(i, _)| positions[*i])
            .collect::<Vec<_>>(),
        [1, 3]
    );
    for (_, control) in controls {
        let c = common(control).unwrap();
        let picture = matches!(control, Control::Picture(_));
        assert_eq!(
            c.treat_as_char,
            if picture {
                inline_picture
            } else {
                inline_table
            }
        );
        assert_eq!(
            c.vertical_offset,
            if variant { (-213_i32) as u32 } else { 1303 }
        );
        assert_eq!(
            c.horizontal_offset,
            if variant { (-567_i32) as u32 } else { 1201 }
        );
        assert_eq!(c.z_order, if variant { -7 } else { 11 });
        assert_eq!(
            c.text_wrap,
            if variant {
                TextWrap::BehindText
            } else {
                TextWrap::InFrontOfText
            }
        );
        assert_eq!(
            c.vert_align,
            if variant {
                VertAlign::Outside
            } else {
                VertAlign::Bottom
            }
        );
        assert_eq!(
            c.horz_align,
            if variant {
                HorzAlign::Inside
            } else {
                HorzAlign::Right
            }
        );
        assert!(!c.flow_with_text);
        assert!(c.allow_overlap && c.affect_line_spacing && c.size_protect);
        assert_eq!(
            (c.width, c.height),
            if picture {
                (1701, 1433)
            } else {
                (if wide { 60003 } else { 9003 }, 7007)
            }
        );
        if let Control::Table(table) = control {
            assert_eq!(
                [
                    table.common.margin.left,
                    table.common.margin.right,
                    table.common.margin.top,
                    table.common.margin.bottom
                ],
                [outer_lr, outer_lr, 283, 283]
            );
            assert_eq!(
                [
                    table.outer_margin_left,
                    table.outer_margin_right,
                    table.outer_margin_top,
                    table.outer_margin_bottom
                ],
                [outer_lr, outer_lr, 283, 283]
            );
            assert_eq!(
                table
                    .cells
                    .iter()
                    .map(|c| (c.width, c.height, c.paragraphs[0].text.as_str()))
                    .collect::<Vec<_>>(),
                if wide {
                    vec![(25001, 7007, "첫 셀🦦"), (35002, 7007, "이웃 셀")]
                } else {
                    vec![(3001, 7007, "첫 셀🦦"), (6002, 7007, "이웃 셀")]
                }
            );
        }
    }
    for at in 0..TEXT.chars().count() {
        let shape =
            &core.document().doc_info.char_shapes[paragraph.char_shape_id_at(at).unwrap() as usize];
        assert_eq!(shape.bold, (1..3).contains(&at));
        assert_eq!(
            shape.text_color,
            if (1..3).contains(&at) { 0x0000_00cc } else { 0 }
        );
    }
}

fn placement(c: &CommonObjAttr) -> Value {
    let mut value = serde_json::to_value(c).unwrap();
    for key in ["ctrl_id", "instance_id", "width", "height"] {
        value.as_object_mut().unwrap().remove(key);
    }
    value
}

fn control_model(
    core: &DocumentCore,
    paragraph: usize,
    path: &[(usize, usize, usize)],
    index: usize,
    control: &Control,
) -> Value {
    let c = common(control).unwrap();
    let mut value = json!({"placement":placement(c),"width":c.width,"height":c.height});
    match control {
        Control::Picture(_) => {
            value["kind"] = json!("picture");
            let bytes = core
                .get_control_image_data_native(0, paragraph, path, index)
                .unwrap();
            assert_eq!(bytes, PNG);
            value["data"] = json!(base64::engine::general_purpose::STANDARD.encode(bytes));
        }
        Control::Table(table) => {
            value["kind"] = json!("table");
            value["rows"] = json!(table.row_count);
            value["cols"] = json!(table.col_count);
            value["outer"] = json!([
                table.outer_margin_left,
                table.outer_margin_right,
                table.outer_margin_top,
                table.outer_margin_bottom
            ]);
            value["cells"] = json!(table.cells.iter().enumerate().map(|(cell, c)| json!({"width":c.width,"height":c.height,"paragraphs":c.paragraphs.iter().enumerate().map(|(p, para)| {
                let mut path = path.to_vec();path.push((index,cell,p));paragraph_model(core,paragraph,&path,para)
            }).collect::<Vec<_>>() })).collect::<Vec<_>>());
        }
        _ => unreachable!(),
    }
    value
}

fn paragraph_model(
    core: &DocumentCore,
    root: usize,
    path: &[(usize, usize, usize)],
    para: &Paragraph,
) -> Value {
    let positions = para.control_text_positions();
    json!({"text":para.text,"styles":(0..para.text.chars().count()).map(|at| {
        let s=&core.document().doc_info.char_shapes[para.char_shape_id_at(at).unwrap() as usize];json!({"bold":s.bold,"italic":s.italic,"color":s.text_color,"size":s.base_size})
    }).collect::<Vec<_>>(),"objects":objects(para).iter().map(|(index,control)| json!({"at":positions[*index],"value":control_model(core,root,path,*index,control)})).collect::<Vec<_>>()})
}

fn first(core: &DocumentCore, paragraph: usize, picture: bool) -> (usize, &Control) {
    objects(&core.document().sections[0].paragraphs[paragraph])
        .into_iter()
        .find(|(_, c)| matches!(c, Control::Picture(_)) == picture)
        .unwrap()
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

fn metadata(html: &str, name: &str) -> Vec<Value> {
    html.split(&format!("{name}=\""))
        .skip(1)
        .map(|part| {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(part.split('"').next().unwrap())
                .unwrap();
            let value: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(value["ctrl_id"], 0);
            assert_eq!(value["instance_id"], 0);
            assert_eq!(value["treat_as_char"], name == V2);
            value
        })
        .collect()
}

fn exported(core: &DocumentCore, picture: bool) -> String {
    let (index, control) = first(core, 0, picture);
    let before = format!("{:?}", core.document());
    let events = core.serialize_event_log();
    let bytes = [
        core.export_hwp_with_adapter().unwrap(),
        core.export_hwpx_native().unwrap(),
    ];
    let html = core.export_control_html_native(0, 0, &[], index).unwrap();
    assert_eq!(format!("{:?}", core.document()), before);
    assert_eq!(core.serialize_event_log(), events);
    assert_eq!(
        [
            core.export_hwp_with_adapter().unwrap(),
            core.export_hwpx_native().unwrap()
        ],
        bytes
    );
    let inline = common(control).unwrap().treat_as_char;
    assert_eq!(metadata(&html, if inline { V2 } else { V1 }).len(), 1);
    assert_eq!(metadata(&html, if inline { V1 } else { V2 }).len(), 0);
    html
}

fn pair_html(core: &DocumentCore) -> String {
    let table = exported(core, false);
    let picture = exported(core, true);
    // 원문 생성 때 넘긴 글과 scalar 앵커를 사용하고, 개체 조각은 공개 exporter에서 읽는다.
    format!("<div data-rhwp-paragraph=\"objects\">가{}<b style=\"color:#cc0000\">🦦나</b>{}<br>끝</div>",fragment(&table),fragment(&picture))
}

fn encoded(common: &CommonObjAttr) -> String {
    let mut common = common.clone();
    common.ctrl_id = 0;
    common.instance_id = 0;
    base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&common).unwrap())
}

fn raw_image(attribute: &str, value: &str) -> String {
    format!("<img src=\"data:image/png;base64,{}\" {attribute}=\"{value}\" width=\"22.68\" height=\"19.106666666666666\">",base64::engine::general_purpose::STANDARD.encode(PNG))
}

fn ids(paragraphs: &[Paragraph]) -> Vec<u32> {
    let mut output = Vec::new();
    for paragraph in paragraphs {
        for control in &paragraph.controls {
            if let Some(c) = common(control) {
                if c.instance_id != 0 {
                    output.push(c.instance_id);
                }
            }
            match control {
                Control::Picture(p) => {
                    if p.instance_id != 0 && p.instance_id != p.common.instance_id {
                        output.push(p.instance_id);
                    }
                }
                Control::Table(t) => {
                    for c in &t.cells {
                        output.extend(ids(&c.paragraphs));
                    }
                }
                _ => {}
            }
        }
    }
    output
}

fn assert_unique(core: &DocumentCore) {
    let values = ids(&core.document().sections[0].paragraphs);
    assert!(!values.is_empty());
    assert_eq!(values.len(), values.iter().collect::<BTreeSet<_>>().len());
    assert!(
        !values.contains(&u32::MAX),
        "검증용 식별자는 저장 모델에 남지 않는다"
    );
}

fn reject_atomically(html: &str) {
    let mut target = blank();
    target.insert_text_native(0, 0, 0, "보존🦦원문").unwrap();
    target
        .apply_char_format_native(0, 0, 1, 3, r##"{"bold":true,"textColor":"#cc0000"}"##)
        .unwrap();
    let model = format!("{:?}", target.document());
    let events = target.serialize_event_log();
    let styles = format!("{:?}", target.document().doc_info);
    let bins = target.document().bin_data_content.len();
    let bytes = [
        target.export_hwp_with_adapter().unwrap(),
        target.export_hwpx_native().unwrap(),
    ];
    let prelude = format!(
        "<p><b>먼저 읽은 글</b>{}</p>",
        raw_image("title", "일반 그림")
    );
    assert!(
        target
            .paste_html_native(0, 0, 2, &format!("{prelude}{html}"))
            .is_err(),
        "{html}"
    );
    assert_eq!(format!("{:?}", target.document()), model);
    assert_eq!(format!("{:?}", target.document().doc_info), styles);
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

#[test]
fn inline_table_v2_preserves_saved_common_cells_and_source() {
    for variant in [false, true] {
        let original = source(variant, false, true, true);
        for origin in saved(&original) {
            assert_source(&origin, variant, false, true, true, 283);
            let (index, control) = first(&origin, 0, false);
            let html = exported(&origin, false);
            let expected = control_model(&origin, 0, &[], index, control);
            let mut target = blank();
            ok(&target.paste_html_native(0, 0, 0, &html).unwrap());
            let (index, control) = first(&target, 0, false);
            assert_eq!(control_model(&target, 0, &[], index, control), expected);
            for (source, target) in saved(&origin).into_iter().zip(saved(&target)) {
                let (s, c) = first(&source, 0, false);
                let (t, d) = first(&target, 0, false);
                assert_eq!(
                    control_model(&target, 0, &[], t, d),
                    control_model(&source, 0, &[], s, c)
                );
                assert_unique(&target);
            }
        }
    }
}

#[test]
fn inline_picture_v2_preserves_saved_common_and_png() {
    for variant in [false, true] {
        let original = source(variant, false, true, true);
        for origin in saved(&original) {
            assert_source(&origin, variant, false, true, true, 283);
            let (index, control) = first(&origin, 0, true);
            let html = exported(&origin, true);
            let expected = control_model(&origin, 0, &[], index, control);
            let mut target = blank();
            ok(&target.paste_html_native(0, 0, 0, &html).unwrap());
            let (index, control) = first(&target, 0, true);
            assert_eq!(control_model(&target, 0, &[], index, control), expected);
            for (source, target) in saved(&origin).into_iter().zip(saved(&target)) {
                let (s, c) = first(&source, 0, true);
                let (t, d) = first(&target, 0, true);
                assert_eq!(
                    control_model(&target, 0, &[], t, d),
                    control_model(&source, 0, &[], s, c)
                );
                assert_unique(&target);
            }
        }
    }
}

#[test]
fn inline_v2_and_floating_v1_keep_one_styled_paragraph_and_scalar_anchors() {
    for inline_table in [false, true] {
        for variant in [false, true] {
            let original = source(variant, false, inline_table, !inline_table);
            for origin in saved(&original) {
                assert_source(&origin, variant, false, inline_table, !inline_table, 283);
                let html = pair_html(&origin);
                assert_eq!(metadata(&html, V1).len(), 1);
                assert_eq!(metadata(&html, V2).len(), 1);
                let mut target = blank();
                ok(&target.paste_html_native(0, 0, 0, &html).unwrap());
                assert_eq!(target.document().sections[0].paragraphs.len(), 1);
                assert_eq!(
                    paragraph_model(
                        &target,
                        0,
                        &[],
                        &target.document().sections[0].paragraphs[0]
                    ),
                    paragraph_model(
                        &origin,
                        0,
                        &[],
                        &origin.document().sections[0].paragraphs[0]
                    )
                );
                for (source, target) in saved(&origin).into_iter().zip(saved(&target)) {
                    assert_eq!(
                        paragraph_model(
                            &target,
                            0,
                            &[],
                            &target.document().sections[0].paragraphs[0]
                        ),
                        paragraph_model(
                            &source,
                            0,
                            &[],
                            &source.document().sections[0].paragraphs[0]
                        )
                    );
                    assert_unique(&target);
                }
            }
        }
    }
}

#[test]
fn nested_whole_table_v2_keeps_children_and_reidentifies_against_existing_objects() {
    let mut original = source(false, false, true, true);
    original.insert_paragraph_native(0, 1).unwrap();
    let outer = ok(&original
        .create_table_ex_native(0, 1, 0, 1, 2, true, Some(&[15003, 15003]), Some(&[14007]))
        .unwrap());
    let outer_index = outer["controlIdx"].as_u64().unwrap() as usize;
    original
        .copy_selection_native(0, 0, 0, 0, TEXT.chars().count())
        .unwrap();
    for cell in 0..2 {
        original
            .paste_internal_in_cell_native(0, 1, outer_index, cell, 0, 0)
            .unwrap();
    }
    for origin in saved(&original) {
        let (s, control) = first(&origin, 1, false);
        let expected = control_model(&origin, 1, &[], s, control);
        let html = origin.export_control_html_native(0, 1, &[], s).unwrap();
        assert_eq!(metadata(&html, V2).len(), 5);
        let mut target = source(true, false, true, true);
        let previous = ids(&target.document().sections[0].paragraphs);
        let old = paragraph_model(
            &target,
            0,
            &[],
            &target.document().sections[0].paragraphs[0],
        );
        target.insert_paragraph_native(0, 1).unwrap();
        ok(&target.paste_html_native(0, 1, 0, &html).unwrap());
        assert_eq!(
            paragraph_model(
                &target,
                0,
                &[],
                &target.document().sections[0].paragraphs[0]
            ),
            old
        );
        let (t, control) = first(&target, 1, false);
        assert_eq!(control_model(&target, 1, &[], t, control), expected);
        assert_unique(&target);
        let copied_ids = ids(&target.document().sections[0].paragraphs[1..]);
        assert!(copied_ids.iter().all(|id| !previous.contains(id)));
        for (source, target) in saved(&origin).into_iter().zip(saved(&target)) {
            let (s, c) = first(&source, 1, false);
            let (t, d) = first(&target, 1, false);
            assert_eq!(
                control_model(&target, 1, &[], t, d),
                control_model(&source, 1, &[], s, c)
            );
            assert_unique(&target);
        }
    }
}

#[test]
fn owned_inline_v2_accepts_an_unmarked_external_data_image() {
    let original = source(false, false, true, true);
    for origin in saved(&original) {
        let html = exported(&origin, true);
        let (s, c) = first(&origin, 0, true);
        let expected = control_model(&origin, 0, &[], s, c);
        let unmarked = raw_image("title", "data-rhwp-common-v2는 속성 값 안의 글");
        let html = format!("<p>가{}🦦{unmarked}끝</p>", fragment(&html));
        let mut target = blank();
        ok(&target.paste_html_native(0, 0, 0, &html).unwrap());
        for result in std::iter::once(&target).chain(saved(&target).iter()) {
            let paragraph = &result.document().sections[0].paragraphs[0];
            let controls = objects(paragraph);
            assert_eq!(paragraph.text, "가🦦끝");
            assert_eq!(controls.len(), 2);
            let positions = paragraph.control_text_positions();
            assert_eq!(
                controls
                    .iter()
                    .map(|(i, _)| positions[*i])
                    .collect::<Vec<_>>(),
                [1, 2]
            );
            if std::ptr::eq(result, &target) {
                assert_eq!(
                    control_model(result, 0, &[], controls[0].0, controls[0].1),
                    expected
                );
            }
            let c = common(controls[1].1).unwrap();
            assert!(c.treat_as_char);
            assert_eq!(c.text_wrap, TextWrap::TopAndBottom);
            assert_unique(result);
        }
    }
}

#[test]
fn inline_table_v2_preserves_placement_inside_the_60003_to_41954_budget() {
    let original = source(false, true, true, true);
    for origin in saved(&original) {
        assert_source(&origin, false, true, true, true, 283);
        let (s, c) = first(&origin, 0, false);
        let mut expected = control_model(&origin, 0, &[], s, c);
        assert_eq!(expected["outer"], json!([283, 283, 283, 283]));
        let html = exported(&origin, false);
        // 기본 문단 여백은 0이며 본문 42520HU에서 원문 바깥 여백 566HU만 예약한다.
        expected["width"] = json!(41954);
        expected["cells"][0]["width"] = json!(17480);
        expected["cells"][1]["width"] = json!(24474);
        let mut target = blank();
        ok(&target.paste_html_native(0, 0, 0, &html).unwrap());
        let (t, d) = first(&target, 0, false);
        assert_eq!(control_model(&target, 0, &[], t, d), expected);
        for (source, target) in saved(&origin).into_iter().zip(saved(&target)) {
            let (s, c) = first(&source, 0, false);
            let mut expected = control_model(&source, 0, &[], s, c);
            expected["width"] = json!(41954);
            expected["cells"][0]["width"] = json!(17480);
            expected["cells"][1]["width"] = json!(24474);
            let (t, d) = first(&target, 0, false);
            assert_eq!(control_model(&target, 0, &[], t, d), expected);
        }
    }
}

#[test]
fn inline_table_v2_reserves_larger_source_outer_margins_without_overflow() {
    let mut original = source(false, true, true, true);
    let mut document = original.document().clone();
    for control in &mut document.sections[0].paragraphs[0].controls {
        if let Control::Table(table) = control {
            table.outer_margin_left = 750;
            table.outer_margin_right = 750;
            table.common.margin.left = 750;
            table.common.margin.right = 750;
        }
    }
    original.set_document(document);
    for origin in saved(&original) {
        assert_source(&origin, false, true, true, true, 750);
        let (s, c) = first(&origin, 0, false);
        let expected_source = control_model(&origin, 0, &[], s, c);
        assert_eq!(expected_source["outer"], json!([750, 750, 283, 283]));
        assert_eq!(expected_source["width"], 60003);
        let html = exported(&origin, false);
        assert!(html.contains("data-rhwp-outer-margin=\"750,750,283,283\""));
        let mut target = blank();
        ok(&target.paste_html_native(0, 0, 0, &html).unwrap());
        // 기본 문단 여백은 0이며 본문 42520HU에서 원문 바깥 여백 1500HU를 예약한다.
        let mut expected = expected_source;
        expected["width"] = json!(41020);
        expected["cells"][0]["width"] = json!(17091);
        expected["cells"][1]["width"] = json!(23929);
        let (t, d) = first(&target, 0, false);
        assert_eq!(control_model(&target, 0, &[], t, d), expected);
        for (source, target) in saved(&origin).into_iter().zip(saved(&target)) {
            let (s, c) = first(&source, 0, false);
            let mut expected = control_model(&source, 0, &[], s, c);
            expected["width"] = json!(41020);
            expected["cells"][0]["width"] = json!(17091);
            expected["cells"][1]["width"] = json!(23929);
            let (t, d) = first(&target, 0, false);
            assert_eq!(control_model(&target, 0, &[], t, d), expected);
        }
    }
}

#[test]
fn inline_v2_invalid_tac_identity_and_size_are_atomic_and_v1_stays_false_only() {
    let original = source(false, false, true, true);
    let (_, control) = first(&original, 0, true);
    let mut valid = serde_json::to_value(common(control).unwrap()).unwrap();
    valid["ctrl_id"] = json!(0);
    valid["instance_id"] = json!(0);
    for (key, value) in [
        ("treat_as_char", json!(false)),
        ("ctrl_id", json!(1)),
        ("instance_id", json!(1)),
        ("width", json!(2147483648_u64)),
        ("height_criterion", json!("Column")),
    ] {
        let mut damaged = valid.clone();
        damaged[key] = value;
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&damaged).unwrap());
        reject_atomically(&raw_image(V2, &encoded));
    }
    reject_atomically(&raw_image(
        V1,
        &base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&valid).unwrap()),
    ));
}

#[test]
fn inline_v2_unclosed_image_empty_table_and_depth_loss_are_atomic() {
    let original = source(false, false, true, true);
    let (_, control) = first(&original, 0, true);
    let value = encoded(common(control).unwrap());
    let image = raw_image(V2, &value);
    reject_atomically(&image.replace(&format!("{V2}=\"{value}\""), &format!("{V2}=\"{value}")));
    reject_atomically(&image.replace(
        &base64::engine::general_purpose::STANDARD.encode(PNG),
        "not-base64!",
    ));
    let (_, table) = first(&original, 0, false);
    let table_value = encoded(common(table).unwrap());
    reject_atomically(&format!("<table {V2}=\"{table_value}\"></table>"));
    let mut deep = image;
    for _ in 0..40 {
        deep = format!("<table><tr><td>{deep}</td></tr></table>");
    }
    reject_atomically(&deep);
}

#[test]
fn inline_v2_quoted_hidden_image_cannot_bypass_the_zero_preflight_count() {
    let original = source(false, false, true, true);
    let (_, control) = first(&original, 0, true);
    let value = encoded(common(control).unwrap());
    let hidden = format!(
        "<span title=\"> <img src='data:image/png;base64,{}' {V2}='{value}'>\">글</span>",
        base64::engine::general_purpose::STANDARD.encode(PNG)
    );
    reject_atomically(&hidden);
}

#[test]
fn owned_common_table_and_picture_cell_paste_rejects_atomically_but_unmarked_html_stays_allowed() {
    for inline in [false, true] {
        let original = source(false, false, inline, inline);
        for origin in saved(&original) {
            for picture in [false, true] {
                let owned = exported(&origin, picture);
                for nested in [false, true] {
                    let cells =
                        "<table><tr><td><p><b>셀 원문🦦</b></p></td><td>이웃 셀</td></tr></table>";
                    let html = if nested {
                        format!("<table><tr><td>{cells}</td><td>바깥 이웃</td></tr></table>")
                    } else {
                        cells.to_owned()
                    };
                    let mut target = blank();
                    ok(&target.paste_html_native(0, 0, 0, &html).unwrap());
                    let (outer_index, outer) = first(&target, 0, false);
                    let mut path = vec![(outer_index, 0, 0)];
                    if nested {
                        let Control::Table(outer) = outer else {
                            unreachable!()
                        };
                        let inner = objects(&outer.cells[0].paragraphs[0]);
                        assert_eq!(inner.len(), 1);
                        path.push((inner[0].0, 0, 0));
                    }
                    assert_eq!(
                        target
                            .get_text_in_cell_by_path(0, 0, &path, 0, 100)
                            .unwrap(),
                        "셀 원문🦦"
                    );
                    let before = format!("{:?}", target.document());
                    let doc_info = format!("{:?}", target.document().doc_info);
                    let bins = target.document().bin_data_content.len();
                    let events = target.serialize_event_log();
                    let bytes = [
                        target.export_hwp_with_adapter().unwrap(),
                        target.export_hwpx_native().unwrap(),
                    ];
                    // 파싱 순서상 앞의 정상 글·그림도 금지된 개체와 함께 부분 적용하지 않는다.
                    let prelude =
                        format!("<p><i>앞선 글</i>{}</p>", raw_image("title", "일반 그림"));
                    let html = format!("{prelude}{}", fragment(&owned));
                    let result = if nested {
                        target.paste_html_in_cell_by_path_native(0, 0, &path, 2, &html)
                    } else {
                        target.paste_html_in_cell_native(0, 0, outer_index, 0, 0, 2, &html)
                    };
                    assert!(
                        result.is_err(),
                        "inline={inline}, picture={picture}, nested={nested}"
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
                    let ordinary = format!("<p>앞{}뒤</p>", raw_image("title", "표시 없는 그림"));
                    let result = if nested {
                        target.paste_html_in_cell_by_path_native(0, 0, &path, 2, &ordinary)
                    } else {
                        target.paste_html_in_cell_native(0, 0, outer_index, 0, 0, 2, &ordinary)
                    };
                    ok(&result.unwrap());
                    assert_eq!(
                        target
                            .get_text_in_cell_by_path(0, 0, &path, 0, 100)
                            .unwrap(),
                        "셀 앞뒤원문🦦"
                    );
                    assert_eq!(target.document().bin_data_content.len(), bins);
                    assert_ne!(
                        target.serialize_event_log(),
                        events,
                        "일반 HTML은 기존 편집 이력에 기록한다"
                    );
                }
            }
        }
    }
}
