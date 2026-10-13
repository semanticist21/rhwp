//! 셀 누름틀 편집은 표·그림의 슬롯과 필드 경계를 두 저장 형식에서 보존한다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::model::paragraph::Paragraph;
use rhwp::model::table::Table;
use serde_json::{json, Value};
use std::collections::BTreeSet;

const PNG: &[u8] = include_bytes!("../../assets/logo/logo-16.png");
const PREFIX: &str = "가🦦";
const SUFFIX: &str = "나다라";
const NEIGHBOR: &str = "이웃🦦\t끝";

fn ok(text: &str) -> Value {
    let value: Value = serde_json::from_str(text).unwrap();
    assert_eq!(value["ok"], true, "{value}");
    value
}

fn table(paragraph: &Paragraph) -> (usize, &Table) {
    paragraph
        .controls
        .iter()
        .enumerate()
        .find_map(|(index, control)| match control {
            Control::Table(table) => Some((index, table.as_ref())),
            _ => None,
        })
        .expect("원문 표가 있어야 한다")
}

fn table_mut(paragraph: &mut Paragraph) -> &mut Table {
    paragraph
        .controls
        .iter_mut()
        .find_map(|control| match control {
            Control::Table(table) => Some(table.as_mut()),
            _ => None,
        })
        .unwrap()
}

fn path(core: &DocumentCore, nested: bool) -> Vec<(usize, usize, usize)> {
    let (outer_index, outer) = table(&core.document().sections[0].paragraphs[1]);
    let mut path = vec![(outer_index, 0, 0)];
    if nested {
        let (inner_index, _) = table(&outer.cells[0].paragraphs[0]);
        path.push((inner_index, 0, 0));
    }
    path
}

fn paragraph_at<'a>(core: &'a DocumentCore, path: &[(usize, usize, usize)]) -> &'a Paragraph {
    let mut paragraph = &core.document().sections[0].paragraphs[1];
    for &(control_index, cell_index, para_index) in path {
        let Control::Table(table) = &paragraph.controls[control_index] else {
            panic!("공개 삽입으로 만든 표 경로여야 한다");
        };
        paragraph = &table.cells[cell_index].paragraphs[para_index];
    }
    paragraph
}

fn saved(core: &DocumentCore) -> [DocumentCore; 2] {
    [
        DocumentCore::from_bytes(&core.export_hwp_with_adapter().unwrap()).unwrap(),
        DocumentCore::from_bytes(&core.export_hwpx_native().unwrap()).unwrap(),
    ]
}

fn unique_ids(paragraph: &Paragraph, ids: &mut BTreeSet<u32>) {
    for control in &paragraph.controls {
        match control {
            Control::Table(table) => {
                assert!(ids.insert(table.common.instance_id), "표 식별자 중복");
                for cell in &table.cells {
                    for paragraph in &cell.paragraphs {
                        unique_ids(paragraph, ids);
                    }
                }
            }
            Control::Picture(picture) => {
                assert!(ids.insert(picture.common.instance_id), "그림 식별자 중복");
            }
            _ => {}
        }
    }
}

fn source(nested: bool, table_before: bool, value: &str) -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.insert_text_native(0, 0, 0, "본문 이웃🦦").unwrap();
    core.insert_paragraph_native(0, 1).unwrap();
    core.insert_text_native(0, 1, 0, &format!("{PREFIX}{SUFFIX}"))
        .unwrap();
    let field = ok(&core
        .insert_click_here_field_at(0, 1, 2, "안내", "메모 <&> 🦦", "셀필드", true)
        .unwrap());
    core.set_field_value_by_id(field["fieldId"].as_u64().unwrap() as u32, value)
        .unwrap();
    let end = 2 + value.chars().count();
    let inserted_table = ok(&core
        .create_table_ex_native(0, 1, end + 1, 1, 1, true, Some(&[3001]), Some(&[2503]))
        .unwrap());
    core.insert_text_in_cell_native(
        0,
        1,
        inserted_table["controlIdx"].as_u64().unwrap() as usize,
        0,
        0,
        0,
        "표",
    )
    .unwrap();
    let inserted_picture = ok(&core
        .insert_picture_native(
            0,
            1,
            1,
            &[],
            PNG,
            1701,
            1433,
            16,
            16,
            "png",
            "셀 그림",
            None,
            None,
        )
        .unwrap());
    core.set_picture_properties_native(
        0,
        1,
        inserted_picture["controlIdx"].as_u64().unwrap() as usize,
        r#"{"treatAsChar":true}"#,
    )
    .unwrap();
    assert_leaf(
        &core,
        1,
        &[],
        false,
        &format!("{PREFIX}{value}{SUFFIX}"),
        value,
        2,
        end,
        1,
        end + 1,
    );
    if table_before {
        // 공개 API로 shape를 만든 뒤 같은 8유닛 GSO 슬롯의 payload만 교환한다.
        // 필드 앞 표 생성의 별도 결함을 fixture에 섞지 않고 raw 위치·필드 범위·ID를 보존한다.
        let mut document = core.document().clone();
        let paragraph = &mut document.sections[0].paragraphs[1];
        let table_index = paragraph
            .controls
            .iter()
            .position(|control| matches!(control, Control::Table(_)))
            .unwrap();
        let picture_index = paragraph
            .controls
            .iter()
            .position(|control| matches!(control, Control::Picture(_)))
            .unwrap();
        assert_eq!(paragraph.ctrl_data_records.len(), paragraph.controls.len());
        paragraph.controls.swap(table_index, picture_index);
        paragraph.ctrl_data_records.swap(table_index, picture_index);
        core.set_document(document);
    }
    // 필드 바깥 글자 모양도 슬롯을 다시 만들 때 함께 밀리거나 덮이면 안 된다.
    core.apply_char_format_native(0, 1, 2, end, r##"{"bold":true,"textColor":"#cc0000"}"##)
        .unwrap();
    core.apply_char_format_native(
        0,
        1,
        end,
        end + 3,
        r##"{"italic":true,"textColor":"#003399"}"##,
    )
    .unwrap();
    assert_leaf(
        &core,
        1,
        &[],
        table_before,
        &format!("{PREFIX}{value}{SUFFIX}"),
        value,
        2,
        end,
        1,
        end + 1,
    );

    core.insert_paragraph_native(0, 2).unwrap();
    let outer = ok(&core
        .create_table_ex_native(0, 2, 0, 1, 2, true, Some(&[27000, 10000]), Some(&[18000]))
        .unwrap());
    core.insert_text_in_cell_native(
        0,
        2,
        outer["controlIdx"].as_u64().unwrap() as usize,
        1,
        0,
        0,
        NEIGHBOR,
    )
    .unwrap();
    if nested {
        core.insert_paragraph_native(0, 3).unwrap();
        let inner = ok(&core
            .create_table_ex_native(0, 3, 0, 1, 2, true, Some(&[17000, 7000]), Some(&[12000]))
            .unwrap());
        core.insert_text_in_cell_native(
            0,
            3,
            inner["controlIdx"].as_u64().unwrap() as usize,
            1,
            0,
            0,
            "중첩 이웃🦦\t뒤",
        )
        .unwrap();
    }

    // 셀 표 생성 API가 없으므로 완성한 문단의 소유 컨테이너만 옮긴다.
    // raw offsets·개체 자리·필드 ID는 손대지 않고 원래 본문에서 같은 문단을 제거한다.
    let mut document = core.document().clone();
    let paragraphs = &mut document.sections[0].paragraphs;
    let inner = nested.then(|| paragraphs.remove(3));
    let leaf = paragraphs.remove(1);
    let outer_table = table_mut(&mut paragraphs[1]);
    if let Some(mut inner) = inner {
        table_mut(&mut inner).cells[0].paragraphs = vec![leaf];
        outer_table.cells[0].paragraphs = vec![inner];
    } else {
        outer_table.cells[0].paragraphs = vec![leaf];
    }
    document.sections[0].raw_stream = None;
    core.set_document(document);
    assert_eq!(core.document().sections[0].paragraphs.len(), 2);
    let mut ids = BTreeSet::new();
    for paragraph in &core.document().sections[0].paragraphs {
        unique_ids(paragraph, &mut ids);
    }
    assert_eq!(ids.len(), if nested { 4 } else { 3 });
    core
}

#[allow(clippy::too_many_arguments)]
fn assert_leaf(
    core: &DocumentCore,
    root: usize,
    path: &[(usize, usize, usize)],
    table_before: bool,
    text: &str,
    value: &str,
    start: usize,
    end: usize,
    before: usize,
    after: usize,
) {
    let paragraph = if path.is_empty() {
        &core.document().sections[0].paragraphs[root]
    } else {
        paragraph_at(core, path)
    };
    assert_eq!(paragraph.text, text, "표·그림 옆 원문 텍스트");
    assert_eq!(paragraph.controls.len(), 3, "필드·표·그림이 하나씩 남는다");
    assert_eq!(paragraph.field_ranges.len(), 1);
    let range = &paragraph.field_ranges[0];
    assert_eq!(
        (range.start_char_idx, range.end_char_idx),
        (start, end),
        "필드 scalar 범위"
    );
    assert_eq!(range.inner_slot_count, 0, "개체는 누름틀 바깥에 있다");
    let Control::Field(field) = &paragraph.controls[range.control_idx] else {
        panic!("필드 범위가 원래 Field를 가리켜야 한다");
    };
    assert_eq!(field.field_name(), Some("셀필드"));
    let fields = core.collect_all_fields();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].field.field_id, field.field_id);
    assert_eq!(
        fields[0].value, value,
        "값 조회는 실제 필드 글자를 반환한다"
    );
    let positions = paragraph.control_text_positions();
    let mut table_seen = false;
    let mut picture_seen = false;
    for (index, control) in paragraph.controls.iter().enumerate() {
        match control {
            Control::Table(table) => {
                table_seen = true;
                assert_eq!(
                    positions[index],
                    if table_before { before } else { after },
                    "표의 원문 자리"
                );
                assert_eq!((table.common.width, table.common.height), (3001, 2503));
                assert!(table.common.treat_as_char);
                assert_eq!(table.cells.len(), 1);
                assert_eq!(table.cells[0].paragraphs[0].text, "표");
            }
            Control::Picture(picture) => {
                picture_seen = true;
                assert_eq!(
                    positions[index],
                    if table_before { after } else { before },
                    "그림의 원문 자리"
                );
                assert_eq!((picture.common.width, picture.common.height), (1701, 1433));
                assert!(picture.common.treat_as_char);
                assert_eq!(
                    core.get_control_image_data_native(0, root, path, index)
                        .unwrap(),
                    PNG
                );
            }
            Control::Field(_) => assert_eq!(positions[index], start, "FIELD_BEGIN 자리"),
            _ => panic!("원문에 없던 컨트롤"),
        }
    }
    assert!(table_seen && picture_seen);
    // 기대 raw 위치는 본문 scalar와 네 개의 8유닛 슬롯에서 독립적으로 계산한다.
    let mut raw = 0_u32;
    let mut offsets = Vec::new();
    for (index, ch) in text.chars().enumerate() {
        raw += [start, end, before, after]
            .iter()
            .filter(|&&at| at == index)
            .count() as u32
            * 8;
        offsets.push(raw);
        raw += if ch == '\t' { 8 } else { ch.len_utf16() as u32 };
    }
    raw += [start, end, before, after]
        .iter()
        .filter(|&&at| at == text.chars().count())
        .count() as u32
        * 8;
    assert_eq!(
        paragraph.char_offsets, offsets,
        "UTF-16·탭·개체·FIELD_END 슬롯"
    );
    assert_eq!(
        paragraph.char_count,
        raw + 1,
        "문단 끝 제어 문자까지 보존한다"
    );
}

fn field_metadata(paragraph: &Paragraph) -> Value {
    let range = &paragraph.field_ranges[0];
    let Control::Field(field) = &paragraph.controls[range.control_idx] else {
        unreachable!();
    };
    json!({
        "field": field,
        "control_index": range.control_idx,
        "end_field_id": range.end_field_id,
        "inner_slot_count": range.inner_slot_count,
    })
}

fn object_metadata(paragraph: &Paragraph) -> Value {
    json!(paragraph.controls.iter().filter_map(|control| match control {
        Control::Table(table) => Some(json!({
            "kind":"table", "common":table.common, "attr":table.attr,
            "outer":[table.outer_margin_left,table.outer_margin_right,table.outer_margin_top,table.outer_margin_bottom],
            "padding":table.padding, "cells":table.cells.iter().map(|cell| json!({
                "row":cell.row,"col":cell.col,"row_span":cell.row_span,"col_span":cell.col_span,
                "width":cell.width,"height":cell.height,"padding":cell.padding,
                "text":cell.paragraphs.iter().map(|paragraph| &paragraph.text).collect::<Vec<_>>(),
            })).collect::<Vec<_>>()
        })),
        Control::Picture(picture) => Some(json!({
            "kind":"picture", "common":picture.common,
            "instance_id":picture.instance_id, "image_attr":picture.image_attr,
        })),
        _ => None,
    }).collect::<Vec<_>>())
}

fn styles(
    core: &DocumentCore,
    paragraph: &Paragraph,
    offsets: impl Iterator<Item = usize>,
) -> Value {
    json!(offsets
        .map(|offset| {
            let id = paragraph.char_shape_id_at(offset).unwrap() as usize;
            serde_json::to_value(&core.document().doc_info.char_shapes[id]).unwrap()
        })
        .collect::<Vec<_>>())
}

fn outside_styles(core: &DocumentCore, paragraph: &Paragraph) -> Value {
    let length = paragraph.text.chars().count();
    styles(core, paragraph, (0..2).chain(length - 3..length))
}

fn neighbor(paragraph: &Paragraph) -> Value {
    json!({"text":paragraph.text,"offsets":paragraph.char_offsets,"shapes":paragraph.char_shapes,
        "para_shape":paragraph.para_shape_id,"controls":paragraph.controls.len(),"fields":paragraph.field_ranges})
}

fn neighbors(core: &DocumentCore, nested: bool) -> Value {
    let paragraphs = &core.document().sections[0].paragraphs;
    let (_, outer) = table(&paragraphs[1]);
    let mut result = vec![
        neighbor(&paragraphs[0]),
        neighbor(&outer.cells[1].paragraphs[0]),
    ];
    assert_eq!(outer.cells[1].paragraphs[0].text, NEIGHBOR);
    if nested {
        let (_, inner) = table(&outer.cells[0].paragraphs[0]);
        result.push(neighbor(&inner.cells[1].paragraphs[0]));
        assert_eq!(inner.cells[1].paragraphs[0].text, "중첩 이웃🦦\t뒤");
    }
    json!(result)
}

#[derive(Clone)]
struct Expected {
    text: String,
    value: String,
    start: usize,
    end: usize,
    before: usize,
    after: usize,
}

impl Expected {
    fn source(value: &str) -> Self {
        let end = 2 + value.chars().count();
        Self {
            text: format!("{PREFIX}{value}{SUFFIX}"),
            value: value.into(),
            start: 2,
            end,
            before: 1,
            after: end + 1,
        }
    }
}

fn assert_expected(core: &DocumentCore, nested: bool, table_before: bool, expected: &Expected) {
    assert_leaf(
        core,
        1,
        &path(core, nested),
        table_before,
        &expected.text,
        &expected.value,
        expected.start,
        expected.end,
        expected.before,
        expected.after,
    );
}

fn assert_unchanged_parts(before: &DocumentCore, after: &DocumentCore, nested: bool) {
    let left = paragraph_at(before, &path(before, nested));
    let right = paragraph_at(after, &path(after, nested));
    assert_eq!(
        field_metadata(right),
        field_metadata(left),
        "필드 전체 메타데이터·종료 식별자"
    );
    assert_eq!(
        object_metadata(right),
        object_metadata(left),
        "이웃 개체 배치·식별자·셀 치수"
    );
    assert_eq!(
        outside_styles(after, right),
        outside_styles(before, left),
        "필드 밖 글자 모양"
    );
    assert_eq!(
        neighbors(after, nested),
        neighbors(before, nested),
        "다른 본문·이웃 셀 불변"
    );
}

fn each_source(value: &str, mut edit: impl FnMut(&mut DocumentCore, bool, bool, &mut Expected)) {
    for nested in [false, true] {
        for table_before in [false, true] {
            let original = source(nested, table_before, value);
            let initial = Expected::source(value);
            assert_expected(&original, nested, table_before, &initial);
            // 편집 전 두 형식 모두 literal 원문 계약을 통과해야 회귀 입력으로 사용한다.
            let source_bytes = [
                original.export_hwp_with_adapter().unwrap(),
                original.export_hwpx_native().unwrap(),
            ];
            for bytes in source_bytes {
                let origin = DocumentCore::from_bytes(&bytes).unwrap();
                assert_expected(&origin, nested, table_before, &initial);
                let before = saved(&origin);
                let mut edited = DocumentCore::from_bytes(&bytes).unwrap();
                assert_expected(&edited, nested, table_before, &initial);
                let mut expected = initial.clone();
                edit(&mut edited, nested, table_before, &mut expected);
                assert_expected(&edited, nested, table_before, &expected);
                assert_unchanged_parts(&origin, &edited, nested);
                for (saved_before, saved_after) in before.iter().zip(saved(&edited).iter()) {
                    assert_expected(saved_after, nested, table_before, &expected);
                    assert_unchanged_parts(saved_before, saved_after, nested);
                }
            }
        }
    }
}

fn insert(core: &mut DocumentCore, nested: bool, offset: usize, text: &str) {
    let path = path(core, nested);
    core.insert_text_in_cell_by_path(0, 1, &path, offset, text)
        .unwrap();
}

#[test]
fn cell_field_set_same_value_preserves_slots_and_both_formats() {
    for value in ["ABCD", "A🦦B\tC"] {
        each_source(value, |core, _, _, expected| {
            let field_id = core.collect_all_fields()[0].field.field_id;
            core.set_field_value_by_id(field_id, &expected.value)
                .unwrap();
        });
    }
}

#[test]
fn cell_field_set_unicode_tab_value_preserves_objects_and_metadata() {
    each_source("ABCD", |core, _, _, expected| {
        let field_id = core.collect_all_fields()[0].field.field_id;
        core.set_field_value_by_id(field_id, "신🦦\t값").unwrap();
        *expected = Expected::source("신🦦\t값");
    });
}

#[test]
fn cell_field_empty_value_then_refill_keeps_nonfield_slots() {
    each_source("A🦦B\tC", |core, nested, table_before, expected| {
        let field_id = core.collect_all_fields()[0].field.field_id;
        core.set_field_value_by_id(field_id, "").unwrap();
        let empty = Expected::source("");
        assert_expected(core, nested, table_before, &empty);
        for reopened in saved(core) {
            assert_expected(&reopened, nested, table_before, &empty);
        }
        core.set_field_value_by_id(field_id, &expected.value)
            .unwrap();
    });
}

#[test]
fn cell_field_inactive_interior_selection_replacement_is_lossless() {
    each_source("ABCD", |core, nested, _, _| {
        let path = path(core, nested);
        core.clear_active_field();
        core.delete_range_in_cell_by_path(0, 1, &path, 0, 3, 0, 5)
            .unwrap();
        insert(core, nested, 3, "BC");
    });
}

#[test]
fn cell_field_active_start_and_end_insert_keep_outside_objects() {
    for at_end in [false, true] {
        each_source("A🦦B\tC", |core, nested, _, expected| {
            let path = path(core, nested);
            let offset = if at_end { expected.end } else { expected.start };
            assert!(core.set_active_field_by_path(0, 1, &path, offset));
            insert(core, nested, offset, "안🦦\t");
            expected.value = if at_end {
                format!("{}안🦦\t", expected.value)
            } else {
                format!("안🦦\t{}", expected.value)
            };
            expected.text = format!("{PREFIX}{}{SUFFIX}", expected.value);
            expected.end += 3;
            expected.after += 3;
        });
    }
}

#[test]
fn cell_field_inactive_start_and_end_insert_stay_outside_field() {
    for at_end in [false, true] {
        each_source("A🦦B\tC", |core, nested, _, expected| {
            core.clear_active_field();
            let offset = if at_end { expected.end } else { expected.start };
            insert(core, nested, offset, "밖🦦\t");
            expected.text = if at_end {
                format!("{PREFIX}{}밖🦦\t{SUFFIX}", expected.value)
            } else {
                format!("{PREFIX}밖🦦\t{}{SUFFIX}", expected.value)
            };
            if !at_end {
                expected.start += 3;
                expected.end += 3;
            }
            expected.after += 3;
        });
    }
}
