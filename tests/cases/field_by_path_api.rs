//! 문단 주소(본문·표 셀·머리말·각주) 필드·자동 번호 넣기. 한컴 입력 > 상용구는 머리말·꼬리말·표 셀·
//! 글상자·주석 안에도 문서 요약 코드와 쪽 번호를 넣는다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::queries::field_query::FieldTarget;
use rhwp::model::control::{AutoNumberType, Control, Field, FieldType};
use rhwp::parser::tags;
use rhwp::wasm_api::HwpDocument;

fn summary(field_id: u32, command: &str) -> Field {
    Field {
        field_type: FieldType::Summary,
        command: command.into(),
        // 한컴 코드처럼 고칠 수 있음(1)만 켜고 dirty(비트 15)는 끈다.
        properties: 1,
        extra_properties: 8,
        field_id,
        ctrl_id: tags::FIELD_SUMMARY,
        ..Default::default()
    }
}

fn summaries(doc: &HwpDocument) -> Vec<FieldTarget> {
    doc.field_targets_native()
        .into_iter()
        .filter(|t| t.field_type == "summary")
        .collect()
}

fn reopen_both(doc: &HwpDocument) -> Vec<HwpDocument> {
    [
        doc.export_hwp_native().unwrap(),
        doc.export_hwpx_native().unwrap(),
    ]
    .iter()
    .map(|bytes| HwpDocument::from_bytes(bytes).unwrap())
    .collect()
}

/// 머리말 문단 경로에 문서 요약 코드와 쪽 번호를 넣으면 목록에 경로와 글이 나오고 저장 뒤에도 남는다.
#[test]
fn header_code_and_page_number_by_path_survive_saves() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.create_header_footer_native(0, true, 0).unwrap();
    doc.insert_text_in_header_footer_native(0, true, 0, 0, 0, "쪽 ")
        .unwrap();
    let info: serde_json::Value =
        serde_json::from_str(&doc.get_header_footer_native(0, true, 0).unwrap()).unwrap();
    let host = info["paraIndex"].as_u64().unwrap() as usize;
    let path = [(info["controlIndex"].as_u64().unwrap() as usize, 0, 0)];
    doc.insert_auto_number_by_path(0, host, &path, 2, AutoNumberType::Page)
        .unwrap();
    // 자동 번호가 든 문단에도 코드를 넣는다(번호 뒤).
    let (start, end) = doc
        .insert_field_by_path(0, host, &path, 3, summary(700, "$author"), "지은이")
        .unwrap();
    assert_eq!((start, end), (3, 6));
    for reopened in std::iter::once(&doc).chain(reopen_both(&doc).iter()) {
        let codes = summaries(reopened);
        assert_eq!(codes.len(), 1);
        assert_eq!(codes[0].cell_path, path);
        assert_eq!(codes[0].value, "지은이");
        assert_eq!(codes[0].command, "$author");
        let para = reopened.document().sections[0].paragraphs[host].controls[path[0].0]
            .header_footer_paragraphs()
            .unwrap()[0]
            .clone();
        assert_eq!(para.text, "쪽  지은이");
        let kinds: Vec<_> = para
            .controls
            .iter()
            .map(|c| match c {
                Control::AutoNumber(a) => format!("{:?}", a.number_type),
                Control::Field(f) => format!("{:?}{}", f.field_type, f.properties),
                _ => "?".into(),
            })
            .collect();
        assert_eq!(kinds, ["Page", "Summary1"]);
    }
}

/// 본문에서 쪽 번호 앞·뒤에 코드를 넣고, 경로로 글을 바꿔도 dirty가 켜지지 않는다.
#[test]
fn body_code_around_auto_number_and_rewrite_keeps_properties() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, "가나").unwrap();
    doc.insert_auto_number_at(0, 0, 1, AutoNumberType::TotalPage)
        .unwrap();
    doc.insert_field_by_path(0, 0, &[], 3, summary(800, "$title"), "제목")
        .unwrap();
    doc.insert_field_by_path(0, 0, &[], 0, summary(801, "$author"), "나")
        .unwrap();
    doc.set_field_text_by_path(0, 0, &[], 800, "새 제목")
        .unwrap();
    for reopened in std::iter::once(&doc).chain(reopen_both(&doc).iter()) {
        let p = &reopened.document().sections[0].paragraphs[0];
        assert_eq!(p.text, "나가 나새 제목");
        let values: Vec<_> = summaries(reopened)
            .into_iter()
            .map(|t| (t.command, t.value))
            .collect();
        assert_eq!(
            values,
            [
                ("$author".into(), "나".into()),
                ("$title".into(), "새 제목".into())
            ]
        );
        for c in &p.controls {
            if let Control::Field(f) = c {
                assert_eq!(f.properties & (1 << 15), 0, "한컴 코드는 dirty를 끈다");
            }
        }
    }
}

/// 표 셀과 각주 문단에도 코드와 전체 쪽수를 넣는다.
#[test]
fn cell_and_footnote_take_codes_and_numbers() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, "본문").unwrap();
    doc.insert_footnote_native(0, 0, 2).unwrap();
    doc.create_table_native(0, 0, 0, 1, 1).unwrap();
    let para = doc.document().sections[0]
        .paragraphs
        .iter()
        .position(|p| p.controls.iter().any(|c| matches!(c, Control::Table(_))))
        .unwrap();
    let table = doc.document().sections[0].paragraphs[para]
        .controls
        .iter()
        .position(|c| matches!(c, Control::Table(_)))
        .unwrap();
    let note_para = doc.document().sections[0]
        .paragraphs
        .iter()
        .position(|p| p.controls.iter().any(|c| matches!(c, Control::Footnote(_))))
        .unwrap();
    let note = doc.document().sections[0].paragraphs[note_para]
        .controls
        .iter()
        .position(|c| matches!(c, Control::Footnote(_)))
        .unwrap();
    let cell = [(table, 0, 0)];
    let note_path = [(note, 0, 0)];
    doc.insert_field_by_path(0, para, &cell, 0, summary(900, "$author"), "셀")
        .unwrap();
    doc.insert_auto_number_by_path(0, para, &cell, 1, AutoNumberType::TotalPage)
        .unwrap();
    let note_len = doc.document().sections[0].paragraphs[note_para].controls[note].clone();
    let Control::Footnote(f) = note_len else {
        unreachable!()
    };
    let at = f.paragraphs[0].text.chars().count();
    doc.insert_field_by_path(0, note_para, &note_path, at, summary(901, "$title"), "주")
        .unwrap();
    for reopened in std::iter::once(&doc).chain(reopen_both(&doc).iter()) {
        let found: Vec<_> = summaries(reopened)
            .into_iter()
            .map(|t| (t.cell_path, t.value))
            .collect();
        assert!(found.contains(&(cell.to_vec(), "셀".into())), "{found:?}");
        assert!(
            found.contains(&(note_path.to_vec(), "주".into())),
            "{found:?}"
        );
    }
}

/// 바탕쪽 문단에도 코드와 쪽 번호·전체 쪽수를 넣고 글을 고친다. 목록은 바탕쪽 번호를 주소로 주고,
/// HWP·HWPX 저장 뒤에도 바탕쪽에 남는다. 한컴 12.30 입력 > 상용구는 바탕쪽 안에도 넣는다.
#[test]
fn master_page_takes_codes_and_numbers() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, "본문").unwrap();
    let created: serde_json::Value = serde_json::from_str(
        &doc.create_master_page_native(0, r#"{"applyTo":0}"#)
            .unwrap(),
    )
    .unwrap();
    let apply_to = created["applyTo"].as_u64().unwrap() as u8;
    // 빈 바탕쪽 문단 맨 앞에 전체 쪽수, 그 앞에 글, 뒤에 쪽 번호와 코드.
    doc.insert_auto_number_in_header_footer_native(
        0,
        true,
        apply_to,
        0,
        0,
        AutoNumberType::TotalPage,
    )
    .unwrap();
    doc.insert_text_in_header_footer_native(0, true, apply_to, 0, 0, "쪽 ")
        .unwrap();
    doc.insert_auto_number_in_header_footer_native(0, true, apply_to, 0, 3, AutoNumberType::Page)
        .unwrap();
    // 바탕쪽 글 문단의 쪽 번호·전체 쪽수는 그 쪽 값으로 그린다("쪽 1 1 …").
    let tree = doc.build_page_render_tree(0).unwrap();
    let mut texts = String::new();
    collect_text(&tree.root, &mut texts);
    assert!(texts.contains("쪽 11"), "{texts:?}");
    let (start, end) = doc
        .insert_field_in_header_footer_native(
            0,
            true,
            apply_to,
            0,
            4,
            summary(950, "$author"),
            "지은이",
        )
        .unwrap();
    assert_eq!((start, end), (4, 7));
    doc.set_field_text_in_header_footer_native(0, true, apply_to, 0, 950, "새 지은이")
        .unwrap();
    for reopened in std::iter::once(&doc).chain(reopen_both(&doc).iter()) {
        let codes = summaries(reopened);
        assert_eq!(codes.len(), 1);
        assert_eq!(
            (codes[0].master, codes[0].para, codes[0].cell_path.len()),
            (Some(0), 0, 0)
        );
        assert_eq!(codes[0].value, "새 지은이");
        let para = &reopened.document().sections[0].section_def.master_pages[0].paragraphs[0];
        assert_eq!(para.text, "쪽   새 지은이");
        let kinds: Vec<_> = para
            .controls
            .iter()
            .filter_map(|c| match c {
                Control::AutoNumber(a) => Some(format!("{:?}", a.number_type)),
                Control::Field(f) => Some(format!("{:?}{}", f.field_type, f.properties)),
                _ => None,
            })
            .collect();
        assert_eq!(kinds, ["TotalPage", "Page", "Summary1"]);
        assert_eq!(reopened.document().sections[0].paragraphs[0].text, "본문");
    }
}

/// 문단 끝 코드 바로 뒤에 친 글은 코드 밖에 들고, 그 문단에 코드를 더 넣는다(본문·머리말).
/// 한컴 12.30에서 "만든 날짜" 코드 뒤에 ", "를 치고 "만든 사람"을 넣으면 둘 다 코드다.
#[test]
fn typing_after_code_at_paragraph_end_still_takes_codes() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, "x").unwrap();
    doc.insert_field_by_path(0, 0, &[], 1, summary(960, "$createtime"), "날짜")
        .unwrap();
    doc.insert_text_native(0, 0, 3, ", ").unwrap();
    doc.insert_field_by_path(0, 0, &[], 5, summary(961, "$author"), "나")
        .unwrap();
    doc.insert_field_by_path(0, 0, &[], 0, summary(962, "$title"), "제목")
        .unwrap();
    doc.create_header_footer_native(0, true, 0).unwrap();
    let info: serde_json::Value =
        serde_json::from_str(&doc.get_header_footer_native(0, true, 0).unwrap()).unwrap();
    let host = info["paraIndex"].as_u64().unwrap() as usize;
    let path = [(info["controlIndex"].as_u64().unwrap() as usize, 0, 0)];
    doc.insert_field_by_path(0, host, &path, 0, summary(963, "$createtime"), "날짜")
        .unwrap();
    doc.insert_text_in_header_footer_native(0, true, 0, 0, 2, ", ")
        .unwrap();
    doc.insert_field_by_path(0, host, &path, 4, summary(964, "$author"), "나")
        .unwrap();
    for reopened in std::iter::once(&doc).chain(reopen_both(&doc).iter()) {
        let body = &reopened.document().sections[0].paragraphs[0];
        assert!(body.text.ends_with("제목x날짜, 나"), "{:?}", body.text);
        let values: Vec<_> = summaries(reopened)
            .into_iter()
            .map(|t| (t.cell_path.len(), t.command, t.value))
            .collect();
        assert_eq!(
            values,
            [
                (0, "$title".into(), "제목".into()),
                (0, "$createtime".into(), "날짜".into()),
                (0, "$author".into(), "나".into()),
                (1, "$createtime".into(), "날짜".into()),
                (1, "$author".into(), "나".into()),
            ]
        );
    }
}

fn collect_text(node: &rhwp::renderer::render_tree::RenderNode, out: &mut String) {
    if let rhwp::renderer::render_tree::RenderNodeType::TextRun(run) = &node.node_type {
        // 자동 번호는 자리표 글자에 표시 글(display_text)로 번호를 그린다.
        out.push_str(run.display_text.as_deref().unwrap_or(&run.text));
    }
    for child in &node.children {
        collect_text(child, out);
    }
}
