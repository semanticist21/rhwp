//! 저장본의 파일 이름 필드는 이름을 다시 정해도 모델·서식·캐럿 주소를 보존한다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::control::{Control, FieldType};
use rhwp::model::paragraph::Paragraph;
use rhwp::wasm_api::HwpDocument;
use serde_json::Value;

const ORIGINAL: &str = "원본.hwp";

fn paragraph(doc: &HwpDocument, header: bool) -> &Paragraph {
    doc.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .find_map(|c| match c {
            Control::Header(h) if header => h.paragraphs.first(),
            Control::Footer(f) if !header => f.paragraphs.first(),
            _ => None,
        })
        .unwrap()
}

fn paths(para: &Paragraph) -> Vec<(usize, usize, u32, u32)> {
    para.field_ranges
        .iter()
        .filter_map(|range| match &para.controls[range.control_idx] {
            Control::Field(field)
                if field.field_type == FieldType::Path && field.command == "$F" =>
            {
                Some((
                    range.start_char_idx,
                    range.end_char_idx,
                    field.field_id,
                    field.properties,
                ))
            }
            _ => None,
        })
        .collect()
}

fn reopen(doc: &HwpDocument, hwpx: bool) -> HwpDocument {
    let bytes = if hwpx {
        doc.export_hwpx().unwrap()
    } else {
        doc.export_hwp().unwrap()
    };
    HwpDocument::from_bytes(&bytes).unwrap()
}

fn fixture() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.set_file_name(ORIGINAL);
    for (index, text) in ["첫 본문", "둘째 본문", "셋째 본문"].iter().enumerate() {
        doc.insert_text_native(0, index, 0, text).unwrap();
        if index < 2 {
            doc.insert_page_break_native(0, index, text.chars().count())
                .unwrap();
        }
    }
    for header in [true, false] {
        doc.create_header_footer_native(0, header, 0).unwrap();
        let mut offset = 0;
        for (text, kind) in [("앞|", 3), ("|중간|", 3), ("|뒤|", 1), ("/", 2)] {
            doc.insert_text_in_header_footer_native(0, header, 0, 0, offset, text)
                .unwrap();
            offset += text.chars().count();
            doc.insert_field_in_hf_native(0, header, 0, 0, offset, kind)
                .unwrap();
            offset += 1;
        }
        for (start, end, json) in [
            (0, 2, r##"{"italic":true,"textColor":"#803090"}"##),
            (
                2,
                3,
                r##"{"bold":true,"underline":true,"textColor":"#008030"}"##,
            ),
            (
                7,
                8,
                r##"{"bold":true,"underline":true,"textColor":"#008030"}"##,
            ),
            (8, 11, r##"{"italic":true,"textColor":"#903010"}"##),
        ] {
            doc.apply_char_format_in_header_footer_native(0, header, 0, 0, start, 0, end, json)
                .unwrap();
        }
    }
    assert_eq!(doc.page_count(), 3);
    doc
}

fn value(doc: &HwpDocument, header: bool, offset: usize) -> Value {
    serde_json::from_str(
        &doc.get_char_properties_in_header_footer_native(0, header, 0, 0, offset)
            .unwrap(),
    )
    .unwrap()
}

fn assert_styles(doc: &HwpDocument, header: bool) {
    let para = paragraph(doc, header);
    assert_eq!(value(doc, header, 0)["italic"], true);
    assert_eq!(value(doc, header, 0)["textColor"], "#803090");
    for (start, end, _, _) in paths(para) {
        for offset in start..end {
            let props = value(doc, header, offset);
            assert_eq!(props["bold"], true, "{header}/{offset} 굵게");
            assert_eq!(props["underline"], true, "{header}/{offset} 밑줄");
            assert_eq!(props["textColor"], "#008030", "{header}/{offset} 색");
        }
    }
    let suffix = para
        .text
        .chars()
        .collect::<Vec<_>>()
        .windows(3)
        .position(|w| w == ['|', '뒤', '|'])
        .unwrap();
    assert_eq!(value(doc, header, suffix)["italic"], true);
    assert_eq!(value(doc, header, suffix)["textColor"], "#903010");
}

fn assert_display(doc: &HwpDocument, name: &str) {
    for page in 0..3 {
        let text = doc.extract_page_text_native(page).unwrap();
        let expected = format!("앞|{name}|중간|{name}|뒤|{}/3", page + 1);
        assert_eq!(text.matches(&expected).count(), 2, "쪽 {page}: {text:?}");
    }
}

fn caret(doc: &HwpDocument, header: bool, offset: usize) -> Value {
    serde_json::from_str(
        &doc.get_cursor_rect_in_header_footer_native(0, header, 0, 0, offset, 0)
            .unwrap(),
    )
    .unwrap()
}

fn assert_field_geometry(doc: &HwpDocument, header: bool) {
    let para = paragraph(doc, header);
    let fields = paths(para);
    let mut spans: Vec<_> = fields
        .iter()
        .map(|&(start, end, _, _)| (start, end))
        .collect();
    spans.extend([
        (0, fields[0].0),
        (fields[0].1, fields[1].0),
        (fields[1].1, para.text.chars().count()),
    ]);
    for (start, end) in spans {
        let left = caret(doc, header, start);
        let right = caret(doc, header, end);
        let selection: Vec<Value> = serde_json::from_str(
            &doc.get_selection_rects_in_header_footer_native(0, header, 0, 0, 0, start, 0, end)
                .unwrap(),
        )
        .unwrap();
        assert!(!selection.is_empty());
        let x = selection[0]["x"].as_f64().unwrap();
        let last = selection.last().unwrap();
        let w = last["x"].as_f64().unwrap() + last["width"].as_f64().unwrap() - x;
        assert!(
            (left["x"].as_f64().unwrap() - x).abs() <= 0.11,
            "시작 캐럿 {left}, 선택 {selection:?}"
        );
        assert!(
            (right["x"].as_f64().unwrap() - x - w).abs() <= 0.11,
            "끝 캐럿 {right}, 선택 {selection:?}"
        );
        let y = left["y"].as_f64().unwrap() + left["height"].as_f64().unwrap() / 2.0;
        for (point, expected) in [(x + 0.01, start), (x + w - 0.01, end)] {
            let hit: Value = serde_json::from_str(
                &doc.hit_test_in_header_footer_native(0, header, point, y)
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(hit["hit"], true);
            assert_eq!(hit["charOffset"], expected, "필드 경계 hit: {hit}");
        }
    }
}

#[test]
fn renamed_saved_fields_project_and_export_without_mutating_live_offsets_or_history() {
    for source_hwpx in [false, true] {
        let mut doc = reopen(&fixture(), source_hwpx);
        assert_display(&doc, ORIGINAL);
        let before = format!("{:?}", doc.document());
        let snapshot = doc.save_snapshot_native();
        let ids: Vec<_> = [true, false]
            .into_iter()
            .flat_map(|h| {
                paths(paragraph(&doc, h))
                    .into_iter()
                    .map(|(_, _, id, properties)| (id, properties))
            })
            .collect();
        for name in ["새로운 보고.hwp", "짧.hwp"] {
            doc.set_file_name(name);
            assert_display(&doc, name);
            for header in [true, false] {
                assert_field_geometry(&doc, header);
                assert_styles(&doc, header);
            }
            for target_hwpx in [false, true] {
                let saved = reopen(&doc, target_hwpx);
                assert_display(&saved, name);
                let saved_ids: Vec<_> = [true, false]
                    .into_iter()
                    .flat_map(|h| {
                        paths(paragraph(&saved, h))
                            .into_iter()
                            .map(|(_, _, id, properties)| (id, properties))
                    })
                    .collect();
                assert_eq!(saved_ids, ids, "필드 ID·속성");
                for header in [true, false] {
                    assert_styles(&saved, header);
                    for (start, end, _, _) in paths(paragraph(&saved, header)) {
                        assert_eq!(
                            paragraph(&saved, header)
                                .text
                                .chars()
                                .skip(start)
                                .take(end - start)
                                .collect::<String>(),
                            name
                        );
                    }
                }
            }
            assert_eq!(
                format!("{:?}", doc.document()),
                before,
                "렌더·저장은 live 원문·필드·서식을 바꾸지 않는다"
            );
        }
        doc.insert_text_in_header_footer_native(0, true, 0, 0, 0, "편집")
            .unwrap();
        doc.restore_snapshot_native(snapshot).unwrap();
        assert_eq!(format!("{:?}", doc.document()), before);
        assert_display(&doc, "짧.hwp");
        assert_field_geometry(&doc, true);
    }
}

#[test]
fn unset_file_name_keeps_stored_values_and_other_path_commands() {
    for source_hwpx in [false, true] {
        let mut doc = reopen(&fixture(), source_hwpx);
        let before = format!("{:?}", doc.document());
        assert_display(&doc, ORIGINAL);
        doc.set_file_name("");
        for target_hwpx in [false, true] {
            assert_display(&reopen(&doc, target_hwpx), ORIGINAL);
        }
        assert_eq!(format!("{:?}", doc.document()), before);

        let mut model = doc.document().clone();
        for ctrl in model.sections[0]
            .paragraphs
            .iter_mut()
            .flat_map(|p| &mut p.controls)
        {
            let paragraphs = match ctrl {
                Control::Header(h) => &mut h.paragraphs,
                Control::Footer(f) => &mut f.paragraphs,
                _ => continue,
            };
            let para = &mut paragraphs[0];
            let index = para.field_ranges[0].control_idx;
            if let Control::Field(field) = &mut para.controls[index] {
                field.command = "$P".to_string();
                field.raw_parameters_xml = None;
                field.parameters = Default::default();
            }
        }
        doc.set_document(model);
        doc.set_file_name("다른.hwp");
        let protected = format!("{:?}", doc.document());
        for target_hwpx in [false, true] {
            let saved = reopen(&doc, target_hwpx);
            for header in [true, false] {
                let para = paragraph(&saved, header);
                let range = para.field_ranges.iter().find(|r| matches!(&para.controls[r.control_idx], Control::Field(field) if field.command == "$P")).unwrap();
                assert_eq!(
                    para.text
                        .chars()
                        .skip(range.start_char_idx)
                        .take(range.end_char_idx - range.start_char_idx)
                        .collect::<String>(),
                    ORIGINAL
                );
            }
        }
        assert_eq!(format!("{:?}", doc.document()), protected);
    }
}

#[test]
fn native_hwp_export_updates_saved_path_fields_in_a_copy() {
    let mut doc = reopen(&fixture(), false);
    let before = format!("{:?}", doc.document());
    doc.set_file_name("직접 저장.hwp");
    let saved = HwpDocument::from_bytes(&doc.export_hwp_native().unwrap()).unwrap();
    assert_display(&saved, "직접 저장.hwp");
    assert_eq!(format!("{:?}", doc.document()), before);
}
