//! 머리말 쪽 번호 자동 번호에 적힌 번호 모양으로 그린다.
//!
//! 한/글 12.30 Mac은 머리말 쪽 번호를 자동 번호(`<hp:autoNum numType="PAGE">`)의
//! `autoNumFormat` 모양으로 그린다(한컴 저장본, 둘째 쪽 `② / II`). 종전 rhwp는 그 모양과
//! 상관없이 아라비아 숫자로 그렸다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::control::{AutoNumberType, Control};
use rhwp::wasm_api::HwpDocument;

const CIRCLED_DIGIT: u8 = 1;

/// 쪽 텍스트의 첫 줄(머리말).
fn first_line(doc: &HwpDocument, page: u32) -> String {
    let text = doc.extract_page_text_native(page).expect("쪽 텍스트");
    text.lines().next().unwrap_or_default().trim().to_string()
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

/// HWPX의 섹션 XML 글을 바꿔 다시 묶는다.
fn rewrite_section(hwpx: &[u8], from: &str, to: &str) -> Vec<u8> {
    use std::io::{Read, Write};
    use zip::write::SimpleFileOptions;

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(hwpx)).expect("HWPX 열기");
    let mut output = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut output);
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).expect("ZIP 엔트리");
            let name = entry.name().expect("ZIP 이름").into_owned();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).expect("ZIP 읽기");
            if name == "Contents/section0.xml" {
                let xml = String::from_utf8(bytes).expect("UTF-8");
                assert!(xml.contains(from), "바꿀 글이 없다");
                bytes = xml.replace(from, to).into_bytes();
            }
            let method = if name == "mimetype" {
                zip::CompressionMethod::Stored
            } else {
                zip::CompressionMethod::Deflated
            };
            zip.start_file(
                name.as_str(),
                SimpleFileOptions::default().compression_method(method),
            )
            .expect("ZIP 엔트리 시작");
            zip.write_all(&bytes).expect("ZIP 쓰기");
        }
        zip.finish().expect("ZIP 닫기");
    }
    output.into_inner()
}

/// 한컴 파일처럼 머리말 쪽 번호 자동 번호에 원 숫자 모양이 적힌 문서도 그 모양으로 그린다.
#[test]
fn stored_header_page_number_shape_is_drawn() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().expect("빈 문서");
    doc.insert_text_native(0, 0, 0, "본문").expect("본문");
    doc.insert_page_break_native(0, 0, 2).expect("쪽 나누기");
    doc.apply_hf_template_native(0, true, 0, 2)
        .expect("가운데 쪽 번호 머리말");
    let hwpx = rewrite_section(
        &doc.export_hwpx().expect("HWPX 저장"),
        r#"numType="PAGE"><hp:autoNumFormat type="DIGIT""#,
        r#"numType="PAGE"><hp:autoNumFormat type="CIRCLED_DIGIT""#,
    );
    let reopened = HwpDocument::from_bytes(&hwpx).expect("다시 열기");
    assert_eq!(header_number_formats(&reopened), vec![CIRCLED_DIGIT]);
    assert_eq!(first_line(&reopened, 0), "①");
    assert_eq!(first_line(&reopened, 1), "②");
}
