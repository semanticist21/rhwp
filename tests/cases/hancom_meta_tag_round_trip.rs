//! 관계형. 한컴 12.30이 태그 넣기로 셀·표에 쓴 HWPX 태그(셀 `hp:subList@metatag`, 표 `hp:metaTag`)는
//! 우리가 열고 HWPX로 다시 저장해도 같은 자리에 남는다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::DocumentCore;
use rhwp::wasm_api::HwpDocument;
use std::io::{Cursor, Read, Write};

fn section_xml(bytes: &[u8]) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("HWPX ZIP");
    let mut xml = String::new();
    archive
        .by_name("Contents/section0.xml")
        .expect("section0")
        .read_to_string(&mut xml)
        .unwrap();
    xml
}

/// HWPX 로 저장한 본문 XML 을 고쳐 다시 연다.
fn rewrite_section(core: &DocumentCore, edit: impl Fn(&str) -> String) -> DocumentCore {
    let bytes = core.export_hwpx_native().unwrap();
    let mut input = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..input.len() {
        let mut entry = input.by_index(i).unwrap();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        if entry.name().expect("HWPX ZIP 엔트리 이름") == "Contents/section0.xml" {
            let xml = String::from_utf8(data).unwrap();
            let edited = edit(&xml);
            assert_ne!(edited, xml, "본문 XML 을 고치지 못했다");
            data = edited.into_bytes();
        }
        output
            .start_file(
                entry.name().expect("HWPX ZIP 엔트리 이름"),
                zip::write::SimpleFileOptions::default().compression_method(entry.compression()),
            )
            .unwrap();
        output.write_all(&data).unwrap();
    }
    DocumentCore::from_bytes(&output.finish().unwrap().into_inner()).unwrap()
}

fn with_table() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.create_table_native(0, 0, 0, 2, 2).unwrap();
    doc
}

#[test]
fn hancom_tags_survive_open_and_save() {
    // 한컴 12.30 저장본(r7y h-tag.hwpx)처럼 첫 셀은 subList 속성, 표는 행 뒤 자식으로 태그를 둔다.
    let doc = with_table();
    let tagged = rewrite_section(&doc, |xml| {
        xml.replacen(
            r#"hasNumRef="0">"#,
            r#"hasNumRef="0" metatag="{&quot;name&quot;:&quot;#칸&quot;}">"#,
            1,
        )
        .replacen(
            "</hp:tr></hp:tbl>",
            r##"</hp:tr><hp:metaTag>{"name":"#표"}</hp:metaTag></hp:tbl>"##,
            1,
        )
    });
    let saved = section_xml(&tagged.export_hwpx_native().unwrap());
    assert!(
        saved.contains(r#"metatag="{&quot;name&quot;:&quot;#칸&quot;}""#),
        "셀 태그가 저장에서 사라졌다"
    );
    assert!(
        saved.contains(r##"<hp:metaTag>{"name":"#표"}</hp:metaTag>"##)
            || saved.contains("<hp:metaTag>{&quot;name&quot;:&quot;#표&quot;}</hp:metaTag>"),
        "표 태그가 저장에서 사라졌다"
    );
}
