//! 관계형. 한컴 12.30 태그 넣기는 셀·셀 블록·표·그림·도형에 태그를 붙이고 HWPX에만 쓴다(HWP로 저장하면
//! 지운다고 묻는다). 정렬은 셀·머리말 안 블록의 문단도 통째로 옮긴다. 기준: 한컴 저장본과 같은 자리에
//! 쓰고 다시 읽으며, 문단은 글과 모양째 차례만 바뀐다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::wasm_api::HwpDocument;
use std::io::{Cursor, Read};

fn json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap()
}

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

/// 2×2 표 하나를 가진 새 문서. (문서, 표 문단, 표 컨트롤)
fn with_table() -> (HwpDocument, usize, usize) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    let made = json(&doc.create_table_native(0, 0, 0, 2, 2).unwrap());
    (
        doc,
        made["paraIdx"].as_u64().unwrap() as usize,
        made["controlIdx"].as_u64().unwrap() as usize,
    )
}

#[test]
fn cell_and_table_tags_round_trip_through_hwpx() {
    let (mut doc, para, ctrl) = with_table();
    let path = [(ctrl, 0, 0)];
    assert!(!doc.has_meta_tags_native());
    doc.set_cell_meta_tag_by_path_native(0, para, &path, 1, "#칸")
        .unwrap();
    doc.set_object_meta_tag_native(0, para, &[], ctrl, "#표")
        .unwrap();
    assert!(doc.has_meta_tags_native());

    let bytes = doc.export_hwpx_native().unwrap();
    let xml = section_xml(&bytes);
    // 한컴 12.30 저장본과 같은 자리: 셀은 subList 끝 속성, 표는 행 뒤 자식.
    assert!(
        xml.contains(r#"metatag="{&quot;name&quot;:&quot;#칸&quot;}""#),
        "{xml}"
    );
    assert!(
        xml.find("<hp:metaTag>").unwrap() > xml.rfind("</hp:tr>").unwrap(),
        "{xml}"
    );

    let back = DocumentCore::from_bytes(&bytes).unwrap();
    assert_eq!(
        back.cell_meta_tag_by_path_native(0, para, &path, 1)
            .unwrap(),
        "#칸"
    );
    assert_eq!(
        back.cell_meta_tag_by_path_native(0, para, &path, 0)
            .unwrap(),
        ""
    );
    assert_eq!(
        back.object_meta_tag_native(0, para, &[], ctrl).unwrap(),
        "#표"
    );
    assert!(back.has_meta_tags_native());

    // HWP5에는 태그 자리가 없다(한컴도 지운다).
    let hwp = DocumentCore::from_bytes(&doc.export_hwp_native().unwrap()).unwrap();
    assert!(!hwp.has_meta_tags_native());

    doc.set_cell_meta_tag_by_path_native(0, para, &path, 1, "")
        .unwrap();
    doc.set_object_meta_tag_native(0, para, &[], ctrl, "")
        .unwrap();
    assert!(!doc.has_meta_tags_native());
    assert!(!section_xml(&doc.export_hwpx_native().unwrap()).contains("metaTag"));
}

#[test]
fn nested_table_cell_tag_is_found_and_saved() {
    let (mut doc, para, ctrl) = with_table();
    let made = json(
        &doc.create_table_in_cell_by_path_native(
            0,
            para,
            &[(ctrl, 0, 0)],
            0,
            1,
            2,
            None,
            None,
            true,
        )
        .unwrap(),
    );
    let inner: Vec<(usize, usize, usize)> = made["cellPath"]
        .as_array()
        .unwrap()
        .iter()
        .map(|step| {
            (
                step["controlIndex"].as_u64().unwrap() as usize,
                step["cellIndex"].as_u64().unwrap() as usize,
                step["cellParaIndex"].as_u64().unwrap() as usize,
            )
        })
        .collect();
    doc.set_cell_meta_tag_by_path_native(0, para, &inner, 1, "#안")
        .unwrap();
    assert!(doc.has_meta_tags_native());
    let back = DocumentCore::from_bytes(&doc.export_hwpx_native().unwrap()).unwrap();
    assert_eq!(
        back.cell_meta_tag_by_path_native(0, para, &inner, 1)
            .unwrap(),
        "#안"
    );
}

#[test]
fn cell_paragraphs_reorder_whole() {
    let (mut doc, para, ctrl) = with_table();
    for (index, text) in ["다", "가", "나"].into_iter().enumerate() {
        if index > 0 {
            let at = [(ctrl, 0, index - 1)];
            doc.split_paragraph_in_cell_by_path(0, para, &at, 1, None)
                .unwrap();
        }
        doc.insert_text_in_cell_by_path(0, para, &[(ctrl, 0, index)], 0, text)
            .unwrap();
    }
    let path = [(ctrl, 0, 0)];
    doc.reorder_paragraphs_by_path_native(0, para, &path, 0, &[1, 2, 0])
        .unwrap();
    let cell_texts = |doc: &DocumentCore| -> Vec<String> {
        let Control::Table(table) = &doc.document().sections[0].paragraphs[para].controls[ctrl]
        else {
            panic!("표");
        };
        table.cells[0]
            .paragraphs
            .iter()
            .map(|p| p.text.clone())
            .collect()
    };
    assert_eq!(cell_texts(&doc), ["가", "나", "다"]);
    for bytes in [
        doc.export_hwp_native().unwrap(),
        doc.export_hwpx_native().unwrap(),
    ] {
        assert_eq!(
            cell_texts(&DocumentCore::from_bytes(&bytes).unwrap()),
            ["가", "나", "다"]
        );
    }
    assert!(doc
        .reorder_paragraphs_by_path_native(0, para, &path, 0, &[0, 0, 1])
        .is_err());
    assert!(doc
        .reorder_paragraphs_by_path_native(0, para, &path, 2, &[1, 0])
        .is_err());
}
