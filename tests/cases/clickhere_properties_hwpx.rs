//! HWPX에서 적재한 누름틀 속성은 원문 캐시·안내문 잔재·입력값을 함께 보존한다.
#![cfg(not(target_arch = "wasm32"))]

use std::io::{Cursor, Read, Write};

use rhwp::model::control::{Control, Field, FieldType, Parameter, ParameterList};
use rhwp::model::document::Document;
use rhwp::model::paragraph::Paragraph;
use rhwp::wasm_api::HwpDocument;
use serde_json::{json, Value};

const FORM: &[u8] = include_bytes!("../../samples/hwpx/form-01.hwpx");
const GUIDE: &str = "새 안내🦦";
const MEMO: &str = "첫 줄\n둘째 줄\t\"인용\" & <메모>";

fn field(doc: &Document, id: u32) -> (&Paragraph, &Field) {
    doc.sections
        .iter()
        .flat_map(|s| &s.paragraphs)
        .find_map(|p| {
            p.controls.iter().find_map(|c| match c {
                Control::Field(f) if f.field_id == id => Some((p, f)),
                _ => None,
            })
        })
        .unwrap()
}

fn field_mut(doc: &mut Document, id: u32) -> &mut Field {
    doc.sections
        .iter_mut()
        .flat_map(|s| &mut s.paragraphs)
        .flat_map(|p| &mut p.controls)
        .find_map(|c| match c {
            Control::Field(f) if f.field_id == id => Some(f),
            _ => None,
        })
        .unwrap()
}

fn props(doc: &HwpDocument, id: u32) -> Value {
    serde_json::from_str(&doc.get_click_here_props(id)).unwrap()
}

fn fields(doc: &HwpDocument) -> Vec<Value> {
    let v: Value = serde_json::from_str(&doc.get_field_list()).unwrap();
    v.as_array()
        .or_else(|| v["fields"].as_array())
        .unwrap()
        .clone()
}

fn state(doc: &HwpDocument, id: u32) -> Value {
    fields(doc)
        .into_iter()
        .find(|v| v["fieldId"] == id)
        .unwrap()
}

fn text(doc: &HwpDocument) -> Vec<String> {
    doc.document()
        .sections
        .iter()
        .flat_map(|s| s.paragraphs.iter().map(|p| p.text.clone()))
        .collect()
}

fn reopen(doc: &HwpDocument, hwpx: bool) -> HwpDocument {
    let bytes = if hwpx {
        doc.export_hwpx().unwrap()
    } else {
        doc.export_hwp_native().unwrap()
    };
    HwpDocument::from_bytes(&bytes).unwrap()
}

fn new_field(value: &str) -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, "앞 뒤").unwrap();
    doc.insert_click_here_field_at(0, 0, 2, "옛 안내", "옛 메모", "이름", true)
        .unwrap();
    doc.split_paragraph_native(0, 0, 3, None).unwrap();
    doc.insert_text_native(0, 1, 0, "보호 문단").unwrap();
    doc.insert_click_here_field_at(0, 1, 2, "이웃 안내", "이웃 메모", "이웃", true)
        .unwrap();
    if !value.is_empty() {
        doc.set_field_value_by_id(1, value).unwrap();
    }
    // 실제 HWP 입력을 HWPX로 변환해 재열기한다. 캐시가 없는 신규 필드만 검사하지 않는다.
    reopen(&reopen(&doc, false), true)
}

fn check_command(params: &ParameterList, command: &str) -> usize {
    params
        .items
        .iter()
        .map(|item| match item {
            Parameter::String {
                name: Some(name),
                value,
                ..
            } if name == "Command" => {
                assert_eq!(value, command);
                1
            }
            Parameter::List(list) => check_command(list, command),
            _ => 0,
        })
        .sum()
}

#[test]
fn loaded_hwpx_properties_preserve_values_siblings_and_snapshot_in_both_formats() {
    for value in ["", "실제 입력🦦"] {
        let mut doc = new_field(value);
        let before = props(&doc, 1);
        let body = text(&doc);
        let sibling = state(&doc, 2);
        let range = state(&doc, 1);
        let original = field(doc.document(), 1).1.clone();
        assert!(original.raw_parameters_xml.is_some());
        let old_snapshot = doc.save_snapshot_native();
        assert_eq!(
            serde_json::from_str::<Value>(&doc.update_click_here_props(
                1,
                GUIDE,
                MEMO,
                "검증 필드",
                false
            ))
            .unwrap()["ok"],
            true
        );
        let wanted =
            json!({"ok":true,"guide":GUIDE,"memo":MEMO,"name":"검증 필드","editable":false});
        assert_eq!(props(&doc, 1), wanted);
        assert_eq!(text(&doc), body);
        assert_eq!(state(&doc, 2), sibling);
        let updated = field(doc.document(), 1).1;
        assert!(updated.raw_parameters_xml.is_none());
        assert_eq!(check_command(&updated.parameters, &updated.command), 1);
        let new_snapshot = doc.save_snapshot_native();
        for hwpx in [false, true] {
            let saved = reopen(&doc, hwpx);
            assert_eq!(props(&saved, 1), wanted, "HWPX={hwpx}");
            assert_eq!(text(&saved), body);
            assert_eq!(state(&saved, 2), sibling);
            let reopened = state(&saved, 1);
            for key in ["fieldId", "value", "startCharIdx", "endCharIdx"] {
                assert_eq!(reopened[key], range[key], "HWPX={hwpx} {key}");
            }
        }
        doc.restore_snapshot_native(old_snapshot).unwrap();
        assert_eq!(props(&doc, 1), before);
        assert_eq!(
            field(doc.document(), 1).1.raw_parameters_xml,
            original.raw_parameters_xml
        );
        assert_eq!(field(doc.document(), 1).1.parameters, original.parameters);
        doc.restore_snapshot_native(new_snapshot).unwrap();
        assert_eq!(props(&doc, 1), wanted);
        assert_eq!(text(&doc), body);
        assert_eq!(
            serde_json::from_str::<Value>(&doc.update_click_here_props(
                u32::MAX,
                "실패",
                "실패",
                "실패",
                true
            ))
            .unwrap()["ok"],
            false
        );
        assert_eq!(props(&doc, 1), wanted);
        assert_eq!(state(&doc, 2), sibling);
        assert_eq!(text(&doc), body);
    }
}

fn form_with_suffix(suffix: &str) -> HwpDocument {
    form_with_text(&format!("여기에 입력{suffix}"))
}

fn form_with_text(text: &str) -> HwpDocument {
    let mut input = zip::ZipArchive::new(Cursor::new(FORM)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..input.len() {
        let mut e = input.by_index(i).unwrap();
        let mut bytes = Vec::new();
        e.read_to_end(&mut bytes).unwrap();
        if e.name() == "Contents/section0.xml" {
            let xml = String::from_utf8(bytes).unwrap();
            let old = "<hp:t>여기에 입력</hp:t>";
            assert_eq!(xml.matches(old).count(), 1);
            bytes = xml
                .replace(old, &format!("<hp:t>{text}</hp:t>"))
                .into_bytes();
        }
        output
            .start_file(
                e.name(),
                zip::write::SimpleFileOptions::default().compression_method(e.compression()),
            )
            .unwrap();
        output.write_all(&bytes).unwrap();
    }
    HwpDocument::from_bytes(&output.finish().unwrap().into_inner()).unwrap()
}

fn form_id(doc: &HwpDocument) -> u32 {
    doc.document()
        .sections
        .iter()
        .flat_map(|s| &s.paragraphs)
        .flat_map(|p| &p.controls)
        .find_map(|c| match c {
            Control::Field(f)
                if f.field_type == FieldType::ClickHere && f.field_name() == Some("myMsg01") =>
            {
                Some(f.field_id)
            }
            _ => None,
        })
        .unwrap()
}

fn raw_value(doc: &Document, id: u32) -> String {
    let (p, _) = field(doc, id);
    let range = p
        .field_ranges
        .iter()
        .find(|r| match &p.controls[r.control_idx] {
            Control::Field(f) => f.field_id == id,
            _ => false,
        })
        .unwrap();
    p.text
        .chars()
        .skip(range.start_char_idx)
        .take(range.end_char_idx - range.start_char_idx)
        .collect()
}

fn value_style(doc: &HwpDocument, id: u32) -> Vec<Value> {
    let f = state(doc, id);
    let sec = f["location"]["sectionIndex"].as_u64().unwrap() as usize;
    let para = f["location"]["paraIndex"].as_u64().unwrap() as usize;
    let start = f["startCharIdx"].as_u64().unwrap() as usize;
    let end = f["endCharIdx"].as_u64().unwrap() as usize;
    (start..end)
        .map(|offset| {
            serde_json::from_str(
                &doc.get_char_properties_at_native(sec, para, offset)
                    .unwrap(),
            )
            .unwrap()
        })
        .collect()
}

#[test]
fn changing_guide_preserves_loaded_clean_values_even_when_the_new_guide_matches() {
    for guide in ["실제 값", ""] {
        // 값 설정 API는 dirty를 세운다. 실제 XML의 dirty=0 입력값으로 삭제 경계를 재현한다.
        let mut doc = form_with_text("실제 값");
        let id = form_id(&doc);
        assert!(!field(doc.document(), id).1.is_dirty());
        assert!(field(doc.document(), id).1.guide_residue.is_none());
        assert_eq!(state(&doc, id)["value"], "실제 값");
        doc.insert_click_here_field_at(0, 0, 0, "이웃 안내", "이웃 메모", "이웃", true)
            .unwrap();
        let siblings: Vec<_> = fields(&doc)
            .into_iter()
            .filter(|f| f["fieldId"] != id)
            .collect();
        assert!(!siblings.is_empty());
        let body = text(&doc);
        let before = state(&doc, id);
        let style = value_style(&doc, id);
        let old = doc.save_snapshot_native();
        assert_eq!(
            serde_json::from_str::<Value>(&doc.update_click_here_props(
                id,
                guide,
                MEMO,
                "검증 필드",
                false
            ))
            .unwrap()["ok"],
            true
        );
        assert!(field(doc.document(), id).1.is_dirty());
        let new = doc.save_snapshot_native();
        for hwpx in [false, true] {
            let saved = reopen(&doc, hwpx);
            assert_eq!(props(&saved, id)["guide"], guide);
            assert_eq!(props(&saved, id)["memo"], MEMO);
            assert_eq!(text(&saved), body, "HWPX={hwpx} 본문 보존");
            assert_eq!(value_style(&saved, id), style, "HWPX={hwpx} 입력 서식");
            let after = state(&saved, id);
            for key in ["fieldId", "value", "startCharIdx", "endCharIdx"] {
                assert_eq!(after[key], before[key], "HWPX={hwpx} {key}");
            }
            assert_eq!(
                fields(&saved)
                    .into_iter()
                    .filter(|f| f["fieldId"] != id)
                    .collect::<Vec<_>>(),
                siblings,
                "HWPX={hwpx} 형제 필드"
            );
        }
        doc.restore_snapshot_native(old).unwrap();
        assert!(!field(doc.document(), id).1.is_dirty());
        assert_eq!(state(&doc, id), before);
        assert_eq!(value_style(&doc, id), style);
        doc.restore_snapshot_native(new).unwrap();
        assert!(field(doc.document(), id).1.is_dirty());
        assert_eq!(text(&doc), body);
        assert_eq!(state(&doc, id)["value"], "실제 값");
    }
}

#[test]
fn empty_guide_normalization_preserves_dirty_whitespace_and_missing_direction_values() {
    for missing_direction in [false, true] {
        let mut doc = form_with_text("  \t");
        let id = form_id(&doc);
        assert_eq!(state(&doc, id)["value"], "  \t");
        if missing_direction {
            // 메모 안에 같은 표기가 있어도 Direction 매개변수가 없으면 안내문으로 인정하지 않는다.
            let f = field_mut(doc.document_mut(), id);
            let memo = "Direction:wstring:0:";
            let inner = format!("HelpState:wstring:{}:{memo}  ", memo.encode_utf16().count());
            f.command = format!("Clickhere:set:{}:{inner}", inner.encode_utf16().count() - 1);
            f.parameters = ParameterList {
                name: None,
                items: vec![Parameter::String {
                    name: Some("Command".into()),
                    value: f.command.clone(),
                    preserve_space: false,
                }],
            };
            f.raw_parameters_xml = None;
            assert!(!f.is_dirty());
        } else {
            doc.set_field_value_by_id(id, "  \t").unwrap();
            assert!(field(doc.document(), id).1.is_dirty());
            doc.update_click_here_props(id, "", MEMO, "검증 필드", true);
        }
        let body = text(&doc);
        let style = value_style(&doc, id);
        for hwpx in [false, true] {
            let saved = reopen(&doc, hwpx);
            assert_eq!(state(&saved, id)["value"], "  \t", "HWPX={hwpx}");
            assert_eq!(text(&saved), body);
            assert_eq!(value_style(&saved, id), style);
            assert!(field(saved.document(), id).1.guide_residue.is_none());
            assert_eq!(field(saved.document(), id).1.is_dirty(), !missing_direction);
        }
    }
}

#[test]
fn actual_initial_guide_residue_keeps_shape_suffix_and_filled_values_in_both_formats() {
    for (suffix, value, guide) in [
        ("", "", GUIDE),
        ("  \t", "", GUIDE),
        ("  \t", "", ""),
        ("  \t", "실제 입력🦦", GUIDE),
    ] {
        let mut doc = form_with_suffix(suffix);
        let id = form_id(&doc);
        let residue = field(doc.document(), id).1.guide_residue.clone().unwrap();
        assert_eq!(residue.text, format!("여기에 입력{suffix}"));
        assert_eq!(residue.char_shape_id, 6);
        assert_eq!(state(&doc, id)["value"], "");
        if !value.is_empty() {
            doc.set_field_value_by_id(id, value).unwrap();
        }
        let body = text(&doc);
        assert_eq!(
            serde_json::from_str::<Value>(&doc.update_click_here_props(
                id,
                guide,
                MEMO,
                "검증 필드",
                false
            ))
            .unwrap()["ok"],
            true
        );
        let changed = field(doc.document(), id).1;
        assert!(changed.parameters.items.iter().any(|item| matches!(item,
            Parameter::String { name: Some(name), value, .. } if name == "Direction" && value == guide
        )), "별도 Direction도 새 안내문으로 저장");
        let changed_residue = changed.guide_residue.as_ref().unwrap();
        assert_eq!(changed_residue.text, format!("{guide}{suffix}"));
        assert_eq!(changed_residue.char_shape_id, residue.char_shape_id);
        assert_eq!(text(&doc), body);
        assert_eq!(state(&doc, id)["value"], value);
        for hwpx in [false, true] {
            let bytes = if hwpx {
                doc.export_hwpx().unwrap()
            } else {
                doc.export_hwp_native().unwrap()
            };
            // 적재 정규화 전 실제 저장 본문을 별도로 검사한다. 빈 값만 비교해 옛 안내문을 숨기지 않는다.
            let raw = rhwp::parse_document(&bytes).unwrap();
            assert_eq!(
                raw_value(&raw, id),
                if value.is_empty() {
                    format!("{guide}{suffix}")
                } else {
                    value.to_string()
                },
                "HWPX={hwpx}"
            );
            let saved = HwpDocument::from_bytes(&bytes).unwrap();
            assert_eq!(
                props(&saved, id),
                json!({"ok":true,"guide":guide,"memo":MEMO,"name":"검증 필드","editable":false})
            );
            assert_eq!(state(&saved, id)["value"], value);
            assert_eq!(text(&saved), body);
            if value.is_empty() && !format!("{guide}{suffix}").is_empty() {
                let after = field(saved.document(), id)
                    .1
                    .guide_residue
                    .as_ref()
                    .unwrap();
                assert_eq!(after.text, changed_residue.text);
                assert_eq!(after.char_shape_id, residue.char_shape_id);
            }
        }
    }
}

#[test]
fn unchanged_command_preserves_cache_and_memo_edit_preserves_guide_residue_and_other_params() {
    let mut doc = form_with_suffix("  \t");
    let id = form_id(&doc);
    let original = field(doc.document(), id).1.clone();
    let before = props(&doc, id);
    assert_eq!(
        serde_json::from_str::<Value>(&doc.update_click_here_props(
            id,
            before["guide"].as_str().unwrap(),
            before["memo"].as_str().unwrap(),
            "이름만",
            false
        ))
        .unwrap()["ok"],
        true
    );
    let f = field(doc.document(), id).1;
    assert_eq!(f.command, original.command);
    assert_eq!(f.raw_parameters_xml, original.raw_parameters_xml);
    assert_eq!(f.parameters, original.parameters);
    assert_eq!(f.guide_residue, original.guide_residue);

    // Command가 중첩되어 있어도 다른 이름의 값·타입·xml:space를 보존한다.
    let f = field_mut(doc.document_mut(), id);
    f.parameters.items.push(Parameter::String {
        name: Some("HelpState".into()),
        value: before["memo"].as_str().unwrap().into(),
        preserve_space: true,
    });
    f.parameters.items.push(Parameter::List(ParameterList {
        name: Some("extra".into()),
        items: vec![
            Parameter::String {
                name: Some("Command".into()),
                value: f.command.clone(),
                preserve_space: true,
            },
            Parameter::String {
                name: Some("Other".into()),
                value: "  원문<&>  ".into(),
                preserve_space: true,
            },
            Parameter::Boolean {
                name: Some("Flag".into()),
                value: true,
                lexical: Some("true".into()),
            },
        ],
    }));
    f.raw_parameters_xml = Some(f.parameters.render_xml("parameters"));
    doc = reopen(&doc, true);
    let params = field(doc.document(), id).1.parameters.clone();
    assert_eq!(
        serde_json::from_str::<Value>(&doc.update_click_here_props(
            id,
            "여기에 입력",
            MEMO,
            "이름만",
            false
        ))
        .unwrap()["ok"],
        true
    );
    let updated = field(doc.document(), id).1;
    assert_eq!(updated.guide_residue, original.guide_residue);
    assert_eq!(check_command(&updated.parameters, &updated.command), 2);
    let mut expected = params;
    for item in &mut expected.items {
        if let Parameter::String {
            name: Some(name),
            value,
            ..
        } = item
        {
            if name == "Command" {
                *value = updated.command.clone();
            } else if name == "HelpState" {
                *value = MEMO.into();
            }
        }
        if let Parameter::List(list) = item {
            if let Parameter::String { value, .. } = &mut list.items[0] {
                *value = updated.command.clone();
            }
        }
    }
    assert_eq!(updated.parameters, expected);
    let saved = reopen(&doc, true);
    assert_eq!(field(saved.document(), id).1.parameters, expected);
    assert_eq!(props(&saved, id)["memo"], MEMO);
    assert_eq!(state(&saved, id)["value"], "");
}
