//! 머리말·꼬리말 마당의 문자 서식은 새 문단과 두 저장 형식에서 보존한다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::control::Control;
use rhwp::model::header_footer::HeaderFooterApply;
use rhwp::model::paragraph::Paragraph;
use rhwp::wasm_api::HwpDocument;
use serde_json::Value;

fn paragraph(doc: &HwpDocument, header: bool, scope: u8) -> &Paragraph {
    let apply = match scope {
        1 => HeaderFooterApply::Even,
        2 => HeaderFooterApply::Odd,
        _ => HeaderFooterApply::Both,
    };
    doc.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|p| &p.controls)
        .find_map(|c| match c {
            Control::Header(h) if header && h.apply_to == apply => h.paragraphs.first(),
            Control::Footer(f) if !header && f.apply_to == apply => f.paragraphs.first(),
            _ => None,
        })
        .expect("머리말·꼬리말 문단")
}

fn props(doc: &HwpDocument, header: bool, scope: u8, offset: usize) -> Value {
    serde_json::from_str(
        &doc.get_char_properties_in_header_footer_native(0, header, scope, 0, offset)
            .expect("글자 모양"),
    )
    .unwrap()
}

fn assert_style(doc: &HwpDocument, header: bool, scope: u8, styled: bool) {
    let p = paragraph(doc, header, scope);
    assert!(!p.text.is_empty());
    for offset in 0..p.text.chars().count() {
        let actual = props(doc, header, scope, offset);
        assert_eq!(actual["bold"], styled, "{header}/{scope}/{offset} 굵게");
        assert_eq!(
            actual["underline"], styled,
            "{header}/{scope}/{offset} 밑줄"
        );
    }
}

fn reopened(doc: &HwpDocument, hwpx: bool) -> HwpDocument {
    let bytes = if hwpx {
        doc.export_hwpx().expect("HWPX 저장")
    } else {
        doc.export_hwp().expect("HWP 저장")
    };
    HwpDocument::from_bytes(&bytes).expect("저장본 다시 열기")
}

#[test]
fn all_template_scopes_apply_bold_and_underline_and_survive_both_formats() {
    for header in [true, false] {
        for scope in 0..=2 {
            for template in 1..=10 {
                let mut doc = HwpDocument::create_empty();
                doc.create_blank_document_native().unwrap();
                doc.set_file_name("마당.hwp");
                doc.apply_hf_template_native(0, header, scope, template)
                    .unwrap();
                let layout = (template - 1) % 5;
                let expected = [
                    "\u{15}",
                    "\u{15}",
                    "\u{15}",
                    "\u{15}\t\u{17}",
                    "\u{17}\t\u{15}",
                ][layout as usize];
                assert_eq!(paragraph(&doc, header, scope).text, expected);
                assert_style(&doc, header, scope, template > 5);
                for hwpx in [false, true] {
                    let saved = reopened(&doc, hwpx);
                    assert_style(&saved, header, scope, template > 5);
                    let p = paragraph(&saved, header, scope);
                    assert!(p
                        .controls
                        .iter()
                        .any(|c| matches!(c, Control::AutoNumber(_))));
                    if layout >= 3 {
                        assert!(p.text.contains("마당.hwp"));
                        assert!(p.controls.iter().any(|c| matches!(c, Control::Field(_))));
                    }
                }
            }
        }
    }
}

#[test]
fn styled_template_replacement_preserves_body_sibling_direct_format_and_fields() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.set_file_name("보호.hwp");
    for (index, text) in ["첫 본문", "둘째 본문", "셋째 본문"].iter().enumerate() {
        doc.insert_text_native(0, index, 0, text).unwrap();
        if index < 2 {
            doc.insert_page_break_native(0, index, text.chars().count())
                .unwrap();
        }
    }
    doc.apply_char_format_native(0, 0, 0, 4, r##"{"italic":true,"textColor":"#803090"}"##)
        .unwrap();
    let body_shapes =
        serde_json::to_value(&doc.document().sections[0].paragraphs[0].char_shapes).unwrap();
    doc.create_header_footer_native(0, true, 2).unwrap();
    doc.insert_text_in_header_footer_native(0, true, 2, 0, 0, "홀수 보호")
        .unwrap();
    doc.apply_char_format_in_header_footer_native(
        0,
        true,
        2,
        0,
        0,
        0,
        5,
        r##"{"italic":true,"textColor":"#305080"}"##,
    )
    .unwrap();
    let sibling_style = props(&doc, true, 2, 0);
    doc.create_header_footer_native(0, false, 0).unwrap();
    doc.insert_text_in_header_footer_native(0, false, 0, 0, 0, "/ · ")
        .unwrap();
    doc.insert_field_in_hf_native(0, false, 0, 0, 0, 1).unwrap();
    doc.insert_field_in_hf_native(0, false, 0, 0, 2, 2).unwrap();
    doc.insert_field_in_hf_native(0, false, 0, 0, 6, 3).unwrap();
    let footer_text = paragraph(&doc, false, 0).text.clone();
    doc.create_header_footer_native(0, true, 0).unwrap();
    doc.insert_text_in_header_footer_native(0, true, 0, 0, 0, "교체 대상")
        .unwrap();
    doc.apply_char_format_in_header_footer_native(
        0,
        true,
        0,
        0,
        0,
        0,
        5,
        r##"{"italic":true,"textColor":"#aa0000"}"##,
    )
    .unwrap();
    doc.apply_hf_template_native(0, true, 0, 9).unwrap();
    assert_eq!(doc.page_count(), 3);
    assert_style(&doc, true, 0, true);
    assert_eq!(
        props(&doc, true, 0, 0)["italic"],
        false,
        "새 마당은 교체 전 직접 모양을 상속하지 않는다"
    );
    assert_eq!(paragraph(&doc, true, 2).text, "홀수 보호");
    assert_eq!(props(&doc, true, 2, 0), sibling_style);
    assert_eq!(paragraph(&doc, false, 0).text, footer_text);
    assert_eq!(
        serde_json::to_value(&doc.document().sections[0].paragraphs[0].char_shapes).unwrap(),
        body_shapes
    );
    for hwpx in [false, true] {
        let saved = reopened(&doc, hwpx);
        assert_eq!(saved.page_count(), 3);
        assert_style(&saved, true, 0, true);
        assert_eq!(paragraph(&saved, true, 2).text, "홀수 보호");
        let actual = props(&saved, true, 2, 0);
        assert_eq!(actual["italic"], true);
        assert_eq!(actual["textColor"], "#305080");
        assert_eq!(
            saved.document().sections[0]
                .paragraphs
                .iter()
                .map(|p| p.text.as_str())
                .collect::<Vec<_>>(),
            ["첫 본문", "둘째 본문", "셋째 본문"]
        );
        for page in 0..3 {
            let drawn = saved.extract_page_text_native(page).unwrap();
            assert!(
                drawn.contains(&format!("{}/3", page + 1)),
                "{hwpx}/{page} 쪽·전체 쪽수: {drawn:?}"
            );
            assert!(
                drawn.contains("보호.hwp"),
                "{hwpx}/{page} 파일 이름: {drawn:?}"
            );
        }
        let body = &saved.document().sections[0].paragraphs[0];
        let shape =
            &saved.document().doc_info.char_shapes[body.char_shapes[0].char_shape_id as usize];
        assert!(shape.italic, "본문 직접 모양 보존");
    }
}
