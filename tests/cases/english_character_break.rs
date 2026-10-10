//! 영문 글자 나눔의 공개 편집·그리기·저장 계약. 단어/하이픈 모드의 기존 배치는 유지한다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::{control::Control, paragraph::Paragraph};
use rhwp::wasm_api::HwpDocument;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use unicode_segmentation::UnicodeSegmentation;

fn value(json: &str) -> Value {
    serde_json::from_str(json).unwrap()
}

fn font(doc: &mut HwpDocument) -> String {
    let id = doc.find_or_create_font_id_native("Courier New");
    assert!(id >= 0);
    // Courier New의 ASCII 전진폭은 1229/2048em이다. 15pt에서 양자화하면
    // 글자와 공백 모두 12px이므로 아래 줄 경계는 측정 구현과 독립적으로 계산한다.
    json!({"fontId":id,"fontSize":1500,"useFontSpace":true}).to_string()
}

fn fixture(text: &str, width_px: u32, indent: i32) -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.set_page_def_native(
        0,
        &json!({"width":width_px*75+1500,"height":30000,
            "marginLeft":750,"marginRight":750,"marginTop":750,"marginBottom":750,
            "marginHeader":0,"marginFooter":0,"marginGutter":0})
        .to_string(),
    )
    .unwrap();
    doc.insert_text_native(0, 0, 0, text).unwrap();
    let font = font(&mut doc);
    doc.apply_char_format_native(0, 0, 0, text.chars().count(), &font)
        .unwrap();
    doc.apply_para_format_native(
        0,
        0,
        &json!({"alignment":"left","marginLeft":0,"marginRight":0,
            "indent":indent,"englishBreakUnit":0})
        .to_string(),
    )
    .unwrap();
    doc
}

fn apply(doc: &mut HwpDocument, unit: u8) {
    doc.apply_para_format(0, 0, &json!({"englishBreakUnit":unit}).to_string())
        .unwrap();
    assert_eq!(
        value(&doc.get_para_properties_at_native(0, 0).unwrap())["englishBreakUnit"],
        unit
    );
}

fn starts(para: &Paragraph) -> Vec<u32> {
    // 구역·단 제어가 차지하는 HWP 유닛은 원문 글자 번호가 아니다.
    // 형식별 줄 축 보정 후 공개 모델의 논리 글자 번호로 비교한다.
    para.line_segs
        .iter()
        .enumerate()
        .map(|(index, _)| para.utf16_pos_to_char_idx(para.line_seg_text_start(index)) as u32)
        .collect()
}

fn body_starts(doc: &HwpDocument) -> Vec<u32> {
    visible_starts(doc, None)
}

fn visible_starts(doc: &HwpDocument, cell: Option<(usize, usize)>) -> Vec<u32> {
    // 편집한 HWPX 문단은 저장 LINE_SEG를 생략할 수 있다. 실제 그린 줄을 비교한다.
    let layout = value(&doc.get_page_text_layout_native(0).unwrap());
    let mut lines: BTreeMap<i64, u32> = BTreeMap::new();
    for run in layout["runs"].as_array().unwrap() {
        let owned = if let Some((parent, control)) = cell {
            run["parentParaIdx"].as_u64() == Some(parent as u64)
                && run["controlIdx"].as_u64() == Some(control as u64)
                && run["cellIdx"] == 0
                && run["cellParaIdx"] == 0
        } else {
            run.get("cellIdx").is_none() && run["paraIdx"] == 0
        };
        let Some(start) = run["charStart"].as_u64() else {
            continue;
        };
        if owned && !run["text"].as_str().unwrap().trim().is_empty() {
            let y = (run["y"].as_f64().unwrap() * 10.0).round() as i64;
            lines
                .entry(y)
                .and_modify(|value| *value = (*value).min(start as u32))
                .or_insert(start as u32);
        }
    }
    lines.into_values().collect()
}

fn reopen(doc: &HwpDocument, hwpx: bool) -> HwpDocument {
    let bytes = if hwpx {
        doc.export_hwpx_native().unwrap()
    } else {
        doc.export_hwp_native().unwrap()
    };
    HwpDocument::from_bytes(&bytes).unwrap()
}

fn caret(doc: &HwpDocument, offset: usize) -> Value {
    value(&doc.get_cursor_rect_native(0, 0, offset).unwrap())
}

// 저장 LINE_SEG뿐 아니라 공개 그리기 API가 방출한 실제 run도 대조한다.
fn rows(doc: &HwpDocument, cell: Option<(usize, usize)>) -> Vec<String> {
    let layout = value(&doc.get_page_text_layout_native(0).unwrap());
    let text = if let Some((parent, control)) = cell {
        let Control::Table(table) =
            &doc.document().sections[0].paragraphs[parent].controls[control]
        else {
            panic!("표 없음");
        };
        table.cells[0].paragraphs[0].text.as_str()
    } else {
        doc.document().sections[0].paragraphs[0].text.as_str()
    };
    let chars = text.chars().collect::<Vec<_>>();
    let mut lines: BTreeMap<i64, Vec<(usize, String)>> = BTreeMap::new();
    for run in layout["runs"].as_array().unwrap() {
        let owned = if let Some((parent, control)) = cell {
            run["parentParaIdx"].as_u64() == Some(parent as u64)
                && run["controlIdx"].as_u64() == Some(control as u64)
                && run["cellIdx"] == 0
                && run["cellParaIdx"] == 0
        } else {
            run.get("cellIdx").is_none() && run["paraIdx"] == 0
        };
        let Some(start) = run["charStart"].as_u64().map(|v| v as usize) else {
            continue;
        };
        if !owned || start >= chars.len() {
            continue;
        }
        let visible: String = run["text"]
            .as_str()
            .unwrap()
            .chars()
            .take(chars.len() - start)
            .collect();
        assert_eq!(
            visible,
            chars[start..start + visible.chars().count()]
                .iter()
                .collect::<String>(),
            "실제 run은 같은 원문 범위를 그려야 한다"
        );
        let y = (run["y"].as_f64().unwrap() * 10.0).round() as i64;
        lines.entry(y).or_default().push((start, visible));
    }
    lines
        .into_values()
        .map(|mut runs| {
            runs.sort_by_key(|(offset, _)| *offset);
            // 일반 줄 끝 공백은 기존 그리기 정책이다. NBSP와 원문 공백은 지우지 않는다.
            runs.into_iter()
                .map(|(_, text)| text)
                .collect::<String>()
                .trim_end_matches(' ')
                .to_owned()
        })
        .collect()
}

fn assert_split_selection(rects: &str) {
    let rects = value(rects);
    let rects = rects.as_array().unwrap();
    assert_eq!(rects.len(), 2, "실제 선택={rects:?}");
    assert!(rects[1]["y"].as_f64().unwrap() > rects[0]["y"].as_f64().unwrap());
    for rect in rects {
        assert!((rect["width"].as_f64().unwrap() - 24.0).abs() < 0.2);
    }
}

#[test]
fn public_edit_changes_actual_rows_caret_selection_and_preserves_history_and_formats() {
    let mut doc = fixture("AAA BBBBB", 72, 0);
    doc.apply_char_format_native(0, 0, 5, 6, r##"{"bold":true,"textColor":"#803090"}"##)
        .unwrap();
    doc.insert_paragraph_native(0, 1).unwrap();
    doc.insert_text_native(0, 1, 0, "SAFE").unwrap();
    let sibling_shape = doc.document().sections[0].paragraphs[1].para_shape_id;
    let original_shapes = serde_json::to_value(&doc.document().doc_info.char_shapes).unwrap();
    let original_para_shape = doc.document().doc_info.para_shapes
        [doc.document().sections[0].paragraphs[0].para_shape_id as usize]
        .clone();
    assert_eq!(body_starts(&doc), [0, 4]);
    assert_eq!(rows(&doc, None), ["AAA", "BBBBB"]);
    let before = doc.save_snapshot_native();
    apply(&mut doc, 2);
    assert_eq!(body_starts(&doc), [0, 6]);
    assert_eq!(rows(&doc, None), ["AAA BB", "BBB"]);
    assert_eq!(
        doc.document().sections[0].paragraphs[1].para_shape_id,
        sibling_shape
    );
    assert_eq!(doc.document().sections[0].paragraphs[1].text, "SAFE");
    assert_eq!(
        serde_json::to_value(&doc.document().doc_info.char_shapes).unwrap(),
        original_shapes
    );
    let mut actual_shape = doc.document().doc_info.para_shapes
        [doc.document().sections[0].paragraphs[0].para_shape_id as usize]
        .clone();
    actual_shape.attr1 = (actual_shape.attr1 & !(3 << 5)) | (original_para_shape.attr1 & (3 << 5));
    actual_shape.raw_data = original_para_shape.raw_data.clone();
    assert_eq!(
        serde_json::to_value(actual_shape).unwrap(),
        serde_json::to_value(original_para_shape).unwrap()
    );
    let first = caret(&doc, 5);
    // 자동 줄 경계 자체는 앞줄 끝 affinity를 가진다. 다음 줄 내부 위치를 확인한다.
    let next = caret(&doc, 7);
    assert!(next["y"].as_f64().unwrap() > first["y"].as_f64().unwrap());
    assert!((next["x"].as_f64().unwrap() - 22.0).abs() < 0.2);
    assert_split_selection(&doc.get_selection_rects(0, 0, 4, 0, 8).unwrap());
    let applied = doc.save_snapshot_native();
    let shapes = doc.document().doc_info.para_shapes.len();
    apply(&mut doc, 2);
    assert_eq!(doc.document().doc_info.para_shapes.len(), shapes);
    assert_eq!(caret(&doc, 7), next);
    for hwpx in [false, true] {
        let saved = reopen(&doc, hwpx);
        assert_eq!(saved.document().sections[0].paragraphs[0].text, "AAA BBBBB");
        assert_eq!(saved.document().sections[0].paragraphs[1].text, "SAFE");
        assert_eq!(
            value(&saved.get_para_properties_at_native(0, 1).unwrap())["englishBreakUnit"],
            0
        );
        assert_eq!(body_starts(&saved), [0, 6]);
        assert_eq!(rows(&saved, None), ["AAA BB", "BBB"]);
        if hwpx {
            assert!(
                saved.document().sections[0].paragraphs[0]
                    .line_segs
                    .is_empty(),
                "저장 줄이 없는 HWPX도 실제 두 줄 선택을 복원해야 한다"
            );
        }
        assert_eq!(caret(&saved, 7), next);
        let props = value(&saved.get_char_properties_at_native(0, 0, 5).unwrap());
        assert_eq!(props["bold"], true);
        assert_eq!(props["textColor"], "#803090");
        assert_eq!(
            value(&saved.get_char_properties_at_native(0, 0, 4).unwrap())["bold"],
            false
        );
        assert_eq!(
            value(&saved.get_para_properties_at_native(0, 0).unwrap())["englishBreakUnit"],
            2
        );
        assert_split_selection(&saved.get_selection_rects(0, 0, 4, 0, 8).unwrap());
    }
    doc.insert_text_native(0, 0, 9, "B").unwrap();
    assert_eq!(rows(&doc, None), ["AAA BB", "BBBB"]);
    doc.restore_snapshot_native(before).unwrap();
    assert_eq!(body_starts(&doc), [0, 4]);
    doc.restore_snapshot_native(applied).unwrap();
    assert_eq!(body_starts(&doc), [0, 6]);
    assert_eq!(caret(&doc, 7), next);
}

#[test]
fn width_indent_and_existing_word_and_literal_hyphen_modes_have_independent_boundaries() {
    for (indent, margin, expected) in [
        (0, 0, vec![0, 6, 12]),
        (900, 0, vec![0, 5, 11, 17]),
        (-900, 0, vec![0, 6, 11, 16]),
        (0, 300, vec![0, 5, 10, 15]),
    ] {
        let mut doc = fixture("AAA BBBBBBBBBBBBBB", 72, indent);
        doc.apply_para_format_native(
            0,
            0,
            &json!({"marginLeft":margin,"marginRight":margin}).to_string(),
        )
        .unwrap();
        apply(&mut doc, 2);
        assert_eq!(
            body_starts(&doc),
            expected,
            "들여쓰기={indent}, 여백={margin}"
        );
        for hwpx in [false, true] {
            let saved = reopen(&doc, hwpx);
            assert_eq!(body_starts(&saved), expected);
            assert_eq!(rows(&saved, None), rows(&doc, None));
        }
    }
    let mut doc = fixture("AAA BB-BB", 84, 0);
    let original = body_starts(&doc);
    assert_eq!(original, [0, 4]);
    let original_rows = rows(&doc, None);
    let original_caret = caret(&doc, 7);
    let original_selection = doc.get_selection_rects(0, 0, 0, 0, 9).unwrap();
    apply(&mut doc, 1);
    assert_eq!(body_starts(&doc), original);
    assert_eq!(rows(&doc, None), original_rows);
    assert_eq!(caret(&doc, 7), original_caret);
    for hwpx in [false, true] {
        let saved = reopen(&doc, hwpx);
        assert_eq!(body_starts(&saved), original);
        assert_eq!(
            saved.get_selection_rects(0, 0, 0, 0, 9).unwrap(),
            original_selection
        );
        assert_eq!(
            value(&saved.get_para_properties_at_native(0, 0).unwrap())["englishBreakUnit"],
            1
        );
    }
    apply(&mut doc, 2);
    assert_eq!(body_starts(&doc), [0, 7]);
    assert_eq!(rows(&doc, None), ["AAA BB-", "BB"]);
    apply(&mut doc, 0);
    assert_eq!(body_starts(&doc), original);
    assert_eq!(rows(&doc, None), original_rows);
    for hwpx in [false, true] {
        let saved = reopen(&doc, hwpx);
        assert_eq!(
            saved.get_selection_rects(0, 0, 0, 0, 9).unwrap(),
            original_selection
        );
    }
}

#[test]
fn punctuation_combining_graphemes_and_nonbreaking_spaces_stay_whole() {
    let mut punctuation = fixture("AAA BC,DE", 72, 0);
    apply(&mut punctuation, 2);
    assert_eq!(body_starts(&punctuation), [0, 5]);
    assert_eq!(rows(&punctuation, None), ["AAA B", "C,DE"]);
    let mut opening = fixture("AAA (BCDE", 72, 0);
    apply(&mut opening, 2);
    assert_eq!(rows(&opening, None), ["AAA (B", "CDE"]);
    let mut combining = fixture("e\u{0301}e\u{0301}e\u{0301}", 20, 0);
    apply(&mut combining, 2);
    assert_eq!(body_starts(&combining), [0, 2, 4]);
    assert_eq!(
        rows(&combining, None),
        ["e\u{0301}", "e\u{0301}", "e\u{0301}"]
    );
    assert_eq!(caret(&combining, 0)["y"], caret(&combining, 1)["y"]);
    let text = combining.document().sections[0].paragraphs[0].text.as_str();
    let mut offset = 0;
    let boundaries = std::iter::once(0)
        .chain(text.graphemes(true).map(|g| {
            offset += g.chars().count();
            offset as u32
        }))
        .collect::<Vec<_>>();
    assert!(body_starts(&combining)
        .iter()
        .all(|start| boundaries.contains(start)));
    // NBSP 삼자 묶음은 36px이다. 첫 줄의 남은 24px보다 크므로 함께 다음 줄로 간다.
    let mut nbsp = fixture("Z A\u{00a0}B C", 48, 0);
    apply(&mut nbsp, 2);
    assert_eq!(body_starts(&nbsp), [0, 2, 6]);
    assert_eq!(rows(&nbsp, None), ["Z", "A\u{00a0}B", "C"]);
    assert_eq!(caret(&nbsp, 3)["y"], caret(&nbsp, 4)["y"]);
    let selection = value(&nbsp.get_selection_rects(0, 0, 2, 0, 5).unwrap());
    assert_eq!(selection.as_array().unwrap().len(), 1);
    assert!(
        (selection[0]["width"].as_f64().unwrap() - 36.0).abs() < 0.2,
        "NBSP 실제 선택={selection}"
    );
    for letter in ["漢", "가"] {
        for glue in ['\u{00a0}', '\u{202f}', '\u{2011}'] {
            let text = format!("Z {letter}{glue}B C");
            let mut mixed = fixture(&text, 48, 0);
            mixed
                .apply_para_format_native(0, 0, r#"{"koreanBreakUnit":1}"#)
                .unwrap();
            apply(&mut mixed, 2);
            let expected = vec!["Z".to_owned(), format!("{letter}{glue}B"), "C".to_owned()];
            assert_eq!(
                rows(&mixed, None),
                expected,
                "혼합 언어 비분리 문자={glue:?}"
            );
            for hwpx in [false, true] {
                assert_eq!(rows(&reopen(&mixed, hwpx), None), expected);
            }
            // 앞선 합법 경계가 없어도 비분리 문자 자체를 새 줄 머리로 만들지 않는다.
            let atom = format!("{letter}{glue}B");
            let mut narrow = fixture(&atom, 20, 0);
            narrow
                .apply_para_format_native(0, 0, r#"{"koreanBreakUnit":1}"#)
                .unwrap();
            apply(&mut narrow, 2);
            assert_eq!(rows(&narrow, None).as_slice(), std::slice::from_ref(&atom));
            for hwpx in [false, true] {
                assert_eq!(
                    rows(&reopen(&narrow, hwpx), None).as_slice(),
                    std::slice::from_ref(&atom)
                );
            }
        }
    }
    for original in [&punctuation, &opening, &combining, &nbsp] {
        for hwpx in [false, true] {
            let saved = reopen(original, hwpx);
            assert_eq!(body_starts(&saved), body_starts(original));
            assert_eq!(rows(&saved, None), rows(original, None));
            assert_eq!(
                saved.document().sections[0].paragraphs[0].text,
                original.document().sections[0].paragraphs[0].text
            );
        }
    }
}

#[test]
fn cell_uses_the_same_breaks_and_caret_and_preserves_neighbor_and_both_formats() {
    let mut doc = fixture("", 300, 0);
    let created = value(&doc.create_table_native(0, 0, 0, 1, 2).unwrap());
    let parent = created["paraIdx"].as_u64().unwrap() as usize;
    let control = created["controlIdx"].as_u64().unwrap() as usize;
    for cell in 0..2 {
        doc.set_cell_properties_native(0, parent, control, cell,
            r#"{"width":5400,"paddingLeft":0,"paddingRight":0,"paddingTop":0,"paddingBottom":0,"applyInnerMargin":true}"#).unwrap();
    }
    doc.insert_text_in_cell_native(0, parent, control, 0, 0, 0, "AAA BBBBB")
        .unwrap();
    doc.insert_text_in_cell_native(0, parent, control, 1, 0, 0, "SAFE")
        .unwrap();
    let font = font(&mut doc);
    doc.apply_char_format_in_cell_native(0, parent, control, 0, 0, 0, 9, &font)
        .unwrap();
    doc.apply_para_format_in_cell_native(
        0,
        parent,
        control,
        0,
        0,
        r#"{"alignment":"left","marginLeft":0,"marginRight":0,"indent":0,"englishBreakUnit":0}"#,
    )
    .unwrap();
    let cell_para = |doc: &HwpDocument, cell: usize| {
        let Control::Table(table) =
            &doc.document().sections[0].paragraphs[parent].controls[control]
        else {
            panic!("표 없음");
        };
        table.cells[cell].paragraphs[0].clone()
    };
    let sibling = serde_json::to_value(cell_para(&doc, 1)).unwrap();
    assert_eq!(starts(&cell_para(&doc, 0)), [0, 4]);
    let before = doc.save_snapshot_native();
    doc.apply_para_format_in_cell_native(0, parent, control, 0, 0, r#"{"englishBreakUnit":2}"#)
        .unwrap();
    assert_eq!(starts(&cell_para(&doc, 0)), [0, 6]);
    assert_eq!(rows(&doc, Some((parent, control))), ["AAA BB", "BBB"]);
    assert_eq!(serde_json::to_value(cell_para(&doc, 1)).unwrap(), sibling);
    let at = |doc: &HwpDocument, offset| {
        value(
            &doc.get_cursor_rect_in_cell_native(0, parent, control, 0, 0, offset)
                .unwrap(),
        )
    };
    let next = at(&doc, 7);
    assert!(next["y"].as_f64().unwrap() > at(&doc, 5)["y"].as_f64().unwrap());
    assert_split_selection(
        &doc.get_selection_rects_in_cell(0, parent as u32, control as u32, 0, 0, 4, 0, 8)
            .unwrap(),
    );
    let applied = doc.save_snapshot_native();
    for hwpx in [false, true] {
        let saved = reopen(&doc, hwpx);
        assert_eq!(visible_starts(&saved, Some((parent, control))), [0, 6]);
        assert_eq!(cell_para(&saved, 0).text, "AAA BBBBB");
        assert_eq!(cell_para(&saved, 1).text, "SAFE");
        assert_eq!(rows(&saved, Some((parent, control))), ["AAA BB", "BBB"]);
        assert_eq!(at(&saved, 7), next);
        assert_eq!(
            value(
                &saved
                    .get_cell_para_properties_at_native(0, parent, control, 0, 0)
                    .unwrap()
            )["englishBreakUnit"],
            2
        );
        assert_split_selection(
            &saved
                .get_selection_rects_in_cell(0, parent as u32, control as u32, 0, 0, 4, 0, 8)
                .unwrap(),
        );
    }
    doc.restore_snapshot_native(before).unwrap();
    assert_eq!(starts(&cell_para(&doc, 0)), [0, 4]);
    doc.restore_snapshot_native(applied).unwrap();
    assert_eq!(at(&doc, 7), next);
    assert_eq!(serde_json::to_value(cell_para(&doc, 1)).unwrap(), sibling);
}

#[test]
fn hwpx_without_saved_lines_selects_two_forced_lines_on_a_middle_page() {
    let mut doc = fixture(&"ABCD\n".repeat(40), 72, 0);
    apply(&mut doc, 2);
    let saved = reopen(&doc, true);
    assert!(saved.document().sections[0].paragraphs[0]
        .line_segs
        .is_empty());
    let layout = value(&saved.get_page_text_layout_native(1).unwrap());
    let mut lines: BTreeMap<i64, (usize, f64)> = BTreeMap::new();
    for run in layout["runs"].as_array().unwrap() {
        if run["paraIdx"] != 0
            || run.get("cellIdx").is_some()
            || run["text"].as_str().unwrap().trim().is_empty()
        {
            continue;
        }
        let y = run["y"].as_f64().unwrap();
        let offset = run["charStart"].as_u64().unwrap() as usize;
        lines
            .entry((y * 10.0).round() as i64)
            .or_insert((offset, y));
    }
    let lines = lines.into_values().collect::<Vec<_>>();
    assert!(lines.len() >= 2);
    assert!(lines[0].0 > 0, "문단 첫 줄이 아닌 중간 쪽에서 선택한다");
    assert_eq!(
        lines[1].0 - lines[0].0,
        5,
        "강제줄바꿈이 있는 실제 두 줄이다"
    );
    let start = lines[0].0 + 1;
    let end = lines[1].0 + 2;
    let selection = value(
        &saved
            .get_selection_rects(0, 0, start as u32, 0, end as u32)
            .unwrap(),
    );
    let rects = selection.as_array().unwrap();
    assert_eq!(rects.len(), 2, "중간 쪽 실제 선택={selection}");
    for (index, width) in [60.0, 24.0].iter().enumerate() {
        assert_eq!(rects[index]["pageIndex"], 1);
        assert!((rects[index]["y"].as_f64().unwrap() - lines[index].1).abs() < 0.2);
        assert!((rects[index]["width"].as_f64().unwrap() - width).abs() < 0.2);
    }
    assert_eq!(
        selection,
        value(
            &doc.get_selection_rects(0, 0, start as u32, 0, end as u32)
                .unwrap()
        ),
        "HWPX의 저장 줄 생략은 원문의 같은 중간 쪽 선택을 바꾸지 않는다"
    );
}
