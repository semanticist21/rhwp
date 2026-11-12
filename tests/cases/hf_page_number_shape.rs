//! 머리말·꼬리말 쪽 번호의 번호 모양.
//!
//! 한/글 12.30 Mac 머리말/꼬리말 도구 상자의 쪽 번호 넣기 ▾는 고른 모양(①·I·가 …)을
//! `autoNumFormat`에 적은 쪽 번호 자동 번호를 넣고, 쪽마다 그 모양으로 그린다
//! (한컴 저장본: `<hp:autoNum numType="PAGE"><hp:autoNumFormat type="CIRCLED_DIGIT"/>`,
//! 둘째 쪽 `② / II`). 종전에는 넣는 API가 모양을 받지 않았고, 한컴 파일의 머리말 쪽 번호도
//! 모양과 상관없이 아라비아 숫자로 그렸다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::control::{AutoNumberType, Control};
use rhwp::wasm_api::HwpDocument;

const CIRCLED_DIGIT: u8 = 1;
const ROMAN_UPPER: u8 = 2;

/// 두 쪽 문서의 빈 머리말에 `① / I` 꼴 쪽 번호 둘을 넣는다.
fn two_page_doc_with_shaped_numbers() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().expect("빈 문서");
    doc.insert_text_native(0, 0, 0, "본문").expect("본문");
    doc.insert_page_break_native(0, 0, 2).expect("쪽 나누기");
    doc.apply_hf_template_native(0, true, 0, 0)
        .expect("빈 머리말");
    let first = doc
        .insert_page_number_in_hf_native(0, true, 0, 0, 0, CIRCLED_DIGIT)
        .expect("원 숫자 쪽 번호");
    assert!(first.contains("\"charOffset\":1"), "{first}");
    doc.insert_text_in_header_footer_native(0, true, 0, 0, 1, " / ")
        .expect("가운데 글");
    doc.insert_page_number_in_hf_native(0, true, 0, 0, 4, ROMAN_UPPER)
        .expect("로마 숫자 쪽 번호");
    doc
}

fn page_text(doc: &HwpDocument, page: u32) -> String {
    doc.extract_page_text_native(page).expect("쪽 텍스트")
}

fn header_number_formats(doc: &HwpDocument) -> Vec<u8> {
    doc.document().sections[0]
        .paragraphs
        .iter()
        .flat_map(|para| para.controls.iter())
        .filter_map(|ctrl| match ctrl {
            Control::Header(header) => Some(header.paragraphs.clone()),
            _ => None,
        })
        .flatten()
        .flat_map(|para| para.controls)
        .filter_map(|ctrl| match ctrl {
            Control::AutoNumber(number) if number.number_type == AutoNumberType::Page => {
                Some(number.format)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn shaped_header_page_numbers_render_per_page() {
    let doc = two_page_doc_with_shaped_numbers();
    assert_eq!(
        header_number_formats(&doc),
        vec![CIRCLED_DIGIT, ROMAN_UPPER]
    );
    assert!(
        page_text(&doc, 0).contains("① / I"),
        "{:?}",
        page_text(&doc, 0)
    );
    assert!(
        page_text(&doc, 1).contains("② / II"),
        "{:?}",
        page_text(&doc, 1)
    );
}

#[test]
fn shaped_header_page_numbers_survive_hwp_and_hwpx() {
    let doc = two_page_doc_with_shaped_numbers();
    for bytes in [
        doc.export_hwp().expect("HWP 저장"),
        doc.export_hwpx().expect("HWPX 저장"),
    ] {
        let reopened = HwpDocument::from_bytes(&bytes).expect("다시 열기");
        assert_eq!(
            header_number_formats(&reopened),
            vec![CIRCLED_DIGIT, ROMAN_UPPER]
        );
        let text = page_text(&reopened, 1);
        assert_eq!(text.lines().next().unwrap_or_default().trim(), "② / II");
    }
}

#[test]
fn unknown_page_number_shape_is_refused() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().expect("빈 문서");
    doc.apply_hf_template_native(0, true, 0, 0)
        .expect("빈 머리말");
    assert!(doc
        .insert_page_number_in_hf_native(0, true, 0, 0, 0, 0x40)
        .is_err());
}
