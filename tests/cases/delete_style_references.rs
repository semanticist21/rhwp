//! 스타일 삭제는 소유 문단의 연결만 보정하고 원문·직접 서식·필드 주소를 보존한다.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeMap;

use rhwp::model::{
    control::{Control, Field, FieldType, HiddenComment, Ruby},
    footnote::{Endnote, Footnote},
    header_footer::{Footer, Header, MasterPage},
    paragraph::Paragraph,
    shape::{Caption, GroupShape, RectangleShape, ShapeObject, TextBox},
    table::{Cell, Table},
};
use rhwp::wasm_api::HwpDocument;
use serde_json::{json, Value};

const NAMES: [&str; 3] = ["삭제 앞 스타일", "삭제할 스타일", "보존할 스타일"];

fn blank() -> (HwpDocument, [u8; 3]) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    let ids = NAMES.map(|name| {
        doc.create_style(&json!({"name":name,"type":0,"nextStyleId":0}).to_string()) as u8
    });
    for (id, next) in [(ids[0], ids[2]), (ids[1], ids[0]), (ids[2], ids[1])] {
        assert!(doc.update_style(u32::from(id), &json!({"nextStyleId":next}).to_string()));
    }
    (doc, ids)
}

fn reopen(doc: &HwpDocument, hwpx: bool) -> HwpDocument {
    let bytes = if hwpx {
        doc.export_hwpx().unwrap()
    } else {
        doc.export_hwp().unwrap()
    };
    HwpDocument::from_bytes(&bytes).unwrap()
}

fn owned_state(doc: &HwpDocument) -> Value {
    Value::Array(
        doc.document()
            .sections
            .iter()
            .map(|section| {
                json!({
                    "paragraphs": section.paragraphs,
                    "master_pages": section.section_def.master_pages,
                })
            })
            .collect(),
    )
}

fn table(doc: &HwpDocument, parent: usize) -> &Table {
    doc.document().sections[0].paragraphs[parent]
        .controls
        .iter()
        .find_map(|c| match c {
            Control::Table(table) => Some(table.as_ref()),
            _ => None,
        })
        .unwrap()
}

fn table_control(doc: &HwpDocument, parent: usize) -> usize {
    doc.document().sections[0].paragraphs[parent]
        .controls
        .iter()
        .position(|control| matches!(control, Control::Table(_)))
        .unwrap()
}

#[test]
fn public_cell_style_keeps_its_name_direct_format_and_value_after_delete_and_save() {
    for source_hwpx in [false, true] {
        let (mut doc, ids) = blank();
        let result: Value =
            serde_json::from_str(&doc.create_table_native(0, 0, 0, 1, 3).unwrap()).unwrap();
        let parent = result["paraIdx"].as_u64().unwrap() as usize;
        let control = result["controlIdx"].as_u64().unwrap() as usize;
        for (cell, id) in ids.iter().enumerate() {
            doc.insert_text_in_cell_native(0, parent, control, cell, 0, 0, "보호 한글🦦")
                .unwrap();
            doc.apply_cell_style_native(0, parent, control, cell, 0, usize::from(*id))
                .unwrap();
        }
        let field: Value = serde_json::from_str(
            &doc.insert_click_here_field_at_in_cell(
                0,
                parent,
                control,
                2,
                0,
                3,
                false,
                "보호 안내",
                "보호 메모",
                "보호 필드",
                true,
            )
            .unwrap(),
        )
        .unwrap();
        let field_id = field["fieldId"].as_u64().unwrap() as u32;
        doc.set_field_value_by_id(field_id, "값").unwrap();
        doc.apply_char_format_in_cell_native(
            0,
            parent,
            control,
            2,
            0,
            0,
            3,
            r##"{"italic":true,"textColor":"#803090"}"##,
        )
        .unwrap();
        doc = reopen(&doc, source_hwpx);
        let before = serde_json::to_value(&table(&doc, parent).cells).unwrap();
        let fields: Value = serde_json::from_str(&doc.get_field_list()).unwrap();
        let field_props: Value = serde_json::from_str(&doc.get_click_here_props(field_id)).unwrap();
        let old = doc.save_snapshot_native();
        assert!(doc.delete_style(u32::from(ids[1])));
        let after = doc.save_snapshot_native();
        let check = |doc: &HwpDocument| {
            let styles = &doc.document().doc_info.styles;
            for (cell, expected) in [usize::from(ids[0]), 0, usize::from(ids[2] - 1)]
                .iter()
                .enumerate()
            {
                let para = &table(doc, parent).cells[cell].paragraphs[0];
                assert_eq!(usize::from(para.style_id), *expected, "셀 {cell}");
                assert_eq!(
                    styles[*expected].local_name,
                    if cell == 1 {
                        styles[0].local_name.as_str()
                    } else {
                        NAMES[cell]
                    }
                );
                assert_eq!(
                    para.text,
                    if cell == 2 {
                        "보호 값한글🦦"
                    } else {
                        "보호 한글🦦"
                    }
                );
            }
            let props: Value = serde_json::from_str(
                &doc.get_cell_char_properties_at_native(
                    0,
                    parent,
                    table_control(doc, parent),
                    2,
                    0,
                    1,
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!(props["italic"], true);
            assert_eq!(props["textColor"], "#803090");
            assert_eq!(
                serde_json::from_str::<Value>(&doc.get_field_list()).unwrap(),
                fields
            );
            assert_eq!(
                serde_json::from_str::<Value>(&doc.get_click_here_props(field_id)).unwrap(),
                field_props
            );
            assert_eq!(styles[usize::from(ids[0])].next_style_id, ids[2] - 1);
            assert_eq!(styles[usize::from(ids[2] - 1)].next_style_id, 0);
        };
        check(&doc);
        for hwpx in [false, true] {
            check(&reopen(&doc, hwpx));
        }
        doc.restore_snapshot_native(old).unwrap();
        assert_eq!(
            serde_json::to_value(&table(&doc, parent).cells).unwrap(),
            before
        );
        doc.restore_snapshot_native(after).unwrap();
        check(&doc);
    }
}

struct Fixture {
    doc: HwpDocument,
    ids: [u8; 3],
    expected: BTreeMap<String, (u8, u8)>,
}

impl Fixture {
    fn new() -> Self {
        let (mut doc, ids) = blank();
        doc.insert_text_native(0, 0, 0, "직접 서식 보호").unwrap();
        doc.apply_char_format_native(0, 0, 0, 2, r##"{"bold":true,"textColor":"#803090"}"##)
            .unwrap();
        doc.apply_para_format_native(0, 0, r#"{"marginLeft":300,"indent":-200}"#)
            .unwrap();
        Self {
            doc,
            ids,
            expected: BTreeMap::new(),
        }
    }

    fn paragraphs(&mut self, owner: &str) -> Vec<Paragraph> {
        (0..3)
            .map(|index| {
                let text = format!("참조/{owner}/{index}");
                let mut p = self.doc.document().sections[0].paragraphs[0].clone();
                p.controls.clear();
                p.field_ranges.clear();
                p.text.clear();
                p.char_offsets.clear();
                p.char_count = 1;
                p.insert_text_at(0, &text);
                p.style_id = self.ids[index];
                self.expected.insert(
                    text,
                    (self.ids[index], [self.ids[0], 0, self.ids[2] - 1][index]),
                );
                p
            })
            .collect()
    }

    fn table(&mut self, owner: &str) -> Table {
        let mut table = Table {
            row_count: 1,
            col_count: 1,
            row_sizes: vec![3600],
            cells: vec![Cell {
                row_span: 1,
                col_span: 1,
                width: 24000,
                height: 3600,
                paragraphs: self.paragraphs(owner),
                ..Default::default()
            }],
            ..Default::default()
        };
        table.common.width = 24000;
        table.common.height = 3600;
        table.rebuild_grid();
        table
    }

    fn textbox(&mut self, owner: &str) -> RectangleShape {
        let mut rectangle = RectangleShape::default();
        rectangle.common.width = 24000;
        rectangle.common.height = 3600;
        rectangle.drawing.text_box = Some(TextBox {
            max_width: 24000,
            paragraphs: self.paragraphs(owner),
            ..Default::default()
        });
        rectangle
    }

    fn populate(&mut self, include_memo: bool) {
        let mut body = self.paragraphs("본문");
        // 표와 글상자 아래에도 표를 두어 문단 번호만으로 평탄화하지 못하게 한다.
        let child = self.table("중첩 표");
        let mut table = self.table("표");
        table.cells[0].paragraphs[0]
            .controls
            .push(Control::Table(Box::new(child)));
        table.caption = Some(Caption {
            paragraphs: self.paragraphs("표 캡션"),
            ..Default::default()
        });
        let mut textbox = self.textbox("글상자");
        let child = self.table("글상자 표");
        textbox.drawing.text_box.as_mut().unwrap().paragraphs[0]
            .controls
            .push(Control::Table(Box::new(child)));
        let group = GroupShape {
            children: vec![ShapeObject::Rectangle(self.textbox("묶음 글상자"))],
            ..Default::default()
        };
        body[0].controls = vec![
            Control::Table(Box::new(table)),
            Control::Shape(Box::new(ShapeObject::Rectangle(textbox))),
            Control::Shape(Box::new(ShapeObject::Group(group))),
            Control::Header(Box::new(Header {
                paragraphs: self.paragraphs("머리말"),
                ..Default::default()
            })),
            Control::Footer(Box::new(Footer {
                paragraphs: self.paragraphs("꼬리말"),
                ..Default::default()
            })),
            Control::Footnote(Box::new(Footnote {
                number: 1,
                paragraphs: self.paragraphs("각주"),
                ..Default::default()
            })),
            Control::Endnote(Box::new(Endnote {
                number: 1,
                paragraphs: self.paragraphs("미주"),
                ..Default::default()
            })),
            Control::HiddenComment(Box::new(HiddenComment {
                paragraphs: self.paragraphs("숨은 설명"),
            })),
        ];
        if include_memo {
            // HWP5의 메모 writer는 별도 꼬리 목록을 쓴다. 이 모델 소유자 검사는 저장 변환과 분리한다.
            body[0].controls.push(Control::Field(Field {
                field_type: FieldType::Memo,
                field_id: 301,
                instance_id: Some(302),
                memo_index: 1,
                memo_paragraphs: self.paragraphs("메모"),
                ..Default::default()
            }));
        }
        for (index, para) in body.iter_mut().enumerate() {
            para.controls.push(Control::Ruby(Ruby {
                main_text: format!("기준{index}"),
                ruby_text: format!("덧말{index}"),
                style_id_ref: u16::from(self.ids[index]),
                sz_ratio: 50,
                ..Default::default()
            }));
        }
        let mut master_paragraphs = self.paragraphs("바탕쪽");
        for (index, para) in master_paragraphs.iter_mut().enumerate() {
            para.controls.push(Control::Ruby(Ruby {
                main_text: format!("바탕기준🦦{index}"),
                ruby_text: format!("바탕덧말{index}"),
                style_id_ref: u16::from(self.ids[index]),
                pos_type: 1,
                sz_ratio: 50,
                option: 0x12345678,
                align: 2,
            }));
        }
        let master = MasterPage {
            paragraphs: master_paragraphs,
            text_width: 24000,
            text_height: 3600,
            text_ref: 1,
            ..Default::default()
        };
        let mut model = self.doc.document().clone();
        model.sections[0].paragraphs = body;
        model.sections[0].section_def.master_pages = vec![master];
        // HWP의 SectionDef 컨트롤과 HWPX의 구역 모델은 같은 바탕쪽을 저장한다.
        let section_def = model.sections[0].section_def.clone();
        model.sections[0].paragraphs[0]
            .controls
            .insert(0, Control::SectionDef(Box::new(section_def)));
        self.doc.set_document(model);
    }

    fn assert_references(&self, doc: &HwpDocument, deleted: bool) {
        let value = owned_state(doc);
        let mut actual = BTreeMap::new();
        collect_references(&value, &mut actual);
        let missing: Vec<_> = self
            .expected
            .keys()
            .filter(|text| !actual.contains_key(*text))
            .collect();
        assert!(missing.is_empty(), "누락된 문단: {missing:?}");
        assert_eq!(actual.len(), self.expected.len());
        for (text, (old, new)) in &self.expected {
            assert_eq!(actual[text], if deleted { *new } else { *old }, "{text}");
            if !deleted || *old != self.ids[1] {
                let index = self.ids.iter().position(|id| id == old).unwrap();
                let id = usize::from(actual[text]);
                assert_eq!(doc.document().doc_info.styles[id].local_name, NAMES[index]);
            }
        }
        let mut ruby_ids = BTreeMap::new();
        collect_ruby_references(&value, &mut ruby_ids);
        assert_eq!(ruby_ids.len(), 6);
        for owner in ["기준", "바탕기준🦦"] {
            for index in 0..3 {
                assert_eq!(
                    ruby_ids[&format!("{owner}{index}")],
                    u16::from(if deleted {
                        [self.ids[0], 0, self.ids[2] - 1][index]
                    } else {
                        self.ids[index]
                    }),
                    "{owner}{index} 덧말"
                );
            }
        }
    }
}

fn collect_references(value: &Value, result: &mut BTreeMap<String, u8>) {
    match value {
        Value::Object(object) => {
            if let (Some(Value::String(text)), Some(id)) =
                (object.get("text"), object.get("style_id"))
            {
                if text.starts_with("참조/") {
                    let id = id.as_u64().unwrap() as u8;
                    if let Some(previous) = result.insert(text.clone(), id) {
                        assert_eq!(previous, id, "구역·컨트롤의 같은 소유자: {text}");
                    }
                }
            }
            for value in object.values() {
                collect_references(value, result);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_references(value, result);
            }
        }
        _ => {}
    }
}

fn collect_ruby_references(value: &Value, result: &mut BTreeMap<String, u16>) {
    match value {
        Value::Object(object) => {
            if let (Some(Value::String(main)), Some(id)) =
                (object.get("main_text"), object.get("style_id_ref"))
            {
                let id = id.as_u64().unwrap() as u16;
                if let Some(previous) = result.insert(main.clone(), id) {
                    assert_eq!(previous, id, "구역·컨트롤의 같은 덧말: {main}");
                }
            }
            for value in object.values() {
                collect_ruby_references(value, result);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_ruby_references(value, result);
            }
        }
        _ => {}
    }
}

// HWP5 표 151의 두 UTF-16 문자열 길이로 스타일 UINT32의 위치를 독립 계산한다.
fn raw_ruby_style_offset(data: &[u8]) -> Option<usize> {
    let id = u32::from_le_bytes(data.get(..4)?.try_into().ok()?);
    if id != rhwp::parser::tags::CTRL_CHAR_OVERLAP {
        return None;
    }
    let main_len = usize::from(u16::from_le_bytes(data.get(4..6)?.try_into().ok()?));
    let sub_start = 6 + main_len * 2;
    let sub_len = usize::from(u16::from_le_bytes(
        data.get(sub_start..sub_start + 2)?.try_into().ok()?,
    ));
    let offset = sub_start + 2 + sub_len * 2 + 12;
    data.get(offset..offset + 8)?;
    Some(offset)
}

fn without_style_ids(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("style_id");
            object.remove("style_id_ref");
            // 바탕쪽의 원본 문단 헤더도 스타일 byte 외에는 그대로 남아야 한다.
            if object.get("tag_id").and_then(Value::as_u64)
                == Some(u64::from(rhwp::parser::tags::HWPTAG_PARA_HEADER))
            {
                if let Some(Value::Array(data)) = object.get_mut("data") {
                    if let Some(style) = data.get_mut(10) {
                        *style = Value::from(0);
                    }
                }
            }
            if object.get("tag_id").and_then(Value::as_u64)
                == Some(u64::from(rhwp::parser::tags::HWPTAG_CTRL_HEADER))
            {
                if let Some(Value::Array(data)) = object.get_mut("data") {
                    let bytes: Vec<_> = data.iter().map(|v| v.as_u64().unwrap() as u8).collect();
                    if let Some(offset) = raw_ruby_style_offset(&bytes) {
                        for byte in &mut data[offset..offset + 4] {
                            *byte = Value::from(0);
                        }
                    }
                }
            }
            for value in object.values_mut() {
                without_style_ids(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                without_style_ids(value);
            }
        }
        _ => {}
    }
}

#[test]
fn every_owned_paragraph_is_remapped_without_touching_content_or_direct_format() {
    let mut fixture = Fixture::new();
    fixture.populate(true);
    fixture.assert_references(&fixture.doc, false);
    let mut before = owned_state(&fixture.doc);
    without_style_ids(&mut before);
    assert!(fixture.doc.delete_style(u32::from(fixture.ids[1])));
    fixture.assert_references(&fixture.doc, true);
    let mut after = owned_state(&fixture.doc);
    without_style_ids(&mut after);
    assert_eq!(after, before);
    let styles = &fixture.doc.document().doc_info.styles;
    assert_eq!(
        styles[usize::from(fixture.ids[0])].next_style_id,
        fixture.ids[2] - 1
    );
    assert_eq!(styles[usize::from(fixture.ids[2] - 1)].next_style_id, 0);
}

#[test]
fn owned_references_survive_delete_in_both_stored_formats_and_repeated_save() {
    for source_hwpx in [false, true] {
        let mut fixture = Fixture::new();
        fixture.populate(false);
        fixture.doc = reopen(&fixture.doc, source_hwpx);
        fixture.assert_references(&fixture.doc, false);
        let mut before = owned_state(&fixture.doc);
        without_style_ids(&mut before);
        assert!(fixture.doc.delete_style(u32::from(fixture.ids[1])));
        fixture.assert_references(&fixture.doc, true);
        let mut after = owned_state(&fixture.doc);
        without_style_ids(&mut after);
        assert_eq!(after, before);
        for hwpx in [false, true] {
            let saved = reopen(&fixture.doc, hwpx);
            fixture.assert_references(&saved, true);
            fixture.assert_references(&reopen(&saved, hwpx), true);
        }
    }
}

#[test]
fn protected_and_invalid_style_deletions_leave_model_and_snapshots_unchanged() {
    let mut fixture = Fixture::new();
    fixture.populate(true);
    let before = owned_state(&fixture.doc);
    let styles = fixture.doc.get_style_list();
    let old = fixture.doc.save_snapshot_native();
    for id in [
        0,
        fixture.doc.document().doc_info.styles.len() as u32,
        u32::MAX,
    ] {
        assert!(!fixture.doc.delete_style(id));
        assert_eq!(fixture.doc.get_style_list(), styles);
        assert_eq!(owned_state(&fixture.doc), before);
    }
    fixture.doc.restore_snapshot_native(old).unwrap();
    fixture.assert_references(&fixture.doc, false);
}

#[test]
fn default_style_next_targets_survive_deletion_with_and_without_original_record_seals() {
    for source in [None, Some(false), Some(true)] {
        let mut doc = HwpDocument::create_empty();
        doc.create_blank_document_native().unwrap();
        if let Some(hwpx) = source {
            doc = reopen(&doc, hwpx);
        }
        let styles = &doc.document().doc_info.styles;
        let base_name = styles[0].local_name.clone();
        let deleted_name = styles[1].local_name.clone();
        let expected: BTreeMap<_, _> = styles
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != 1)
            .map(|(_, style)| {
                let next = &styles[usize::from(style.next_style_id)].local_name;
                (
                    style.local_name.clone(),
                    if next == &deleted_name {
                        base_name.clone()
                    } else {
                        next.clone()
                    },
                )
            })
            .collect();
        let check = |doc: &HwpDocument| {
            let styles = &doc.document().doc_info.styles;
            assert_eq!(styles.len(), expected.len());
            for style in styles {
                let next = &styles[usize::from(style.next_style_id)].local_name;
                assert_eq!(
                    next, &expected[&style.local_name],
                    "{}의 다음 스타일",
                    style.local_name
                );
            }
        };
        assert!(doc.delete_style(1));
        check(&doc);
        for hwpx in [false, true] {
            check(&reopen(&doc, hwpx));
        }
    }
}

#[test]
fn master_page_ruby_raw_style_word_preserves_tails_and_unrelated_or_incomplete_records() {
    use rhwp::model::document::RawRecord;
    use rhwp::parser::{control::parse_control, tags};

    let mut fixture = Fixture::new();
    fixture.populate(false);
    fixture.doc = reopen(&fixture.doc, false);
    let mut model = fixture.doc.document().clone();
    let records = &mut model.sections[0].section_def.extra_child_records;
    let index = records
        .iter()
        .position(|record| {
            record.tag_id == tags::HWPTAG_CTRL_HEADER
                && raw_ruby_style_offset(&record.data).is_some()
                && matches!(parse_control(tags::CTRL_CHAR_OVERLAP, &record.data[4..], &[]),
                    Control::Ruby(ruby) if ruby.main_text == "바탕기준🦦1")
        })
        .unwrap();
    let offset = raw_ruby_style_offset(&records[index].data).unwrap();
    // 알려지지 않은 꼬리 바이트와 잘린 원본을 재직렬화하거나 추측해서 덮지 않는다.
    records[index]
        .data
        .extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
    let original = records[index].data.clone();
    let extra_start = records.len();
    let mut other_control = original.clone();
    other_control[..4].copy_from_slice(&tags::CTRL_TCPS.to_le_bytes());
    let mut out_of_range = original.clone();
    out_of_range[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    for (tag_id, data) in [
        (tags::HWPTAG_CTRL_HEADER, other_control),
        (tags::HWPTAG_CTRL_DATA, original.clone()),
        (tags::HWPTAG_CTRL_HEADER, original[..3].to_vec()),
        (tags::HWPTAG_CTRL_HEADER, original[..offset + 4].to_vec()),
        (tags::HWPTAG_CTRL_HEADER, out_of_range),
    ] {
        records.push(RawRecord {
            tag_id,
            level: 4,
            data,
        });
    }
    let before = records.clone();
    let section_def = model.sections[0].section_def.clone();
    let control = model.sections[0].paragraphs[0]
        .controls
        .iter_mut()
        .find(|control| matches!(control, Control::SectionDef(_)))
        .unwrap();
    *control = Control::SectionDef(Box::new(section_def));
    fixture.doc.set_document(model);
    assert!(fixture.doc.delete_style(u32::from(fixture.ids[1])));
    let records = &fixture.doc.document().sections[0]
        .section_def
        .extra_child_records;
    assert_eq!(records.len(), before.len());
    assert_eq!(records[index].data[..offset], original[..offset]);
    assert_eq!(records[index].data[offset..offset + 4], 0u32.to_le_bytes());
    assert_eq!(records[index].data[offset + 4..], original[offset + 4..]);
    assert_eq!(
        serde_json::to_value(&records[extra_start..]).unwrap(),
        serde_json::to_value(&before[extra_start..]).unwrap()
    );
}
