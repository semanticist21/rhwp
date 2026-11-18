//! #6956: 텍스트와 표를 감싼 형광펜 3쌍을 HWPX/HWP5 왕복에서 보존한다.
//! 한컴 2022 변환 대조: PARA_RANGE_TAG 종류 2, COLORREF BGR, 끝 위치 exclusive.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::hyperlink::HyperlinkTarget;
use rhwp::document_core::DocumentCore;
use rhwp::model::document::Document;
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
use std::io::{Cursor, Read, Write};

fn sample() -> Document {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/samples/issue6956/3024739-exposure-algorithm-markpen.hwpx"
    ))
    .expect("공개 재현물");
    rhwp::parser::parse_document(&bytes).expect("HWPX 원본")
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

fn assert_marks(doc: &Document, color: &str) {
    let bytes = rhwp::serializer::hwpx::serialize_hwpx(doc).expect("HWPX 저장");
    let xml = section_xml(&bytes);
    assert_eq!(
        xml.matches("<hp:markpenBegin").count(),
        3,
        "표를 감싼 1쌍도 보존"
    );
    assert_eq!(xml.matches("<hp:markpenEnd").count(), 3);
    assert_eq!(
        xml.matches(&format!("<hp:markpenBegin color=\"{color}\"/>"))
            .count(),
        3
    );
    let reloaded = rhwp::parser::parse_document(&bytes).expect("HWPX 재적재");
    let table_para = reloaded
        .sections
        .iter()
        .flat_map(|s| &s.paragraphs)
        .find(|p| p.text.is_empty() && p.markpen_marks.len() == 2)
        .expect("표를 감싼 문단");
    assert_eq!(table_para.markpen_marks[0].utf16_pos, Some(0));
    assert_eq!(table_para.markpen_marks[1].utf16_pos, Some(8));
    assert!(table_para.markpen_marks[0].color.is_some());
    assert!(table_para.markpen_marks[1].color.is_none());
}

#[test]
fn issue_6956_text_and_table_markpen_survive_two_hwpx_roundtrips() {
    let doc = sample();
    assert_marks(&doc, "#FFFFFF");
    let bytes = rhwp::serializer::hwpx::serialize_hwpx(&doc).unwrap();
    let again = rhwp::parser::parse_document(&bytes).unwrap();
    assert_marks(&again, "#FFFFFF");
}

#[test]
fn issue_6956_markpen_does_not_consume_the_text_axis() {
    for para in sample().sections.iter().flat_map(|s| &s.paragraphs) {
        assert!(
            !para.text.contains('\u{0007}'),
            "sentinel이 텍스트에 새면 안 된다"
        );
        for mark in &para.markpen_marks {
            assert!(mark.char_idx <= para.text.chars().count());
            assert!(mark.utf16_pos.unwrap() < para.char_count);
        }
    }
}

#[test]
fn issue_6956_hwp5_colorref_and_table_span_survive_roundtrip() {
    let mut doc = sample();
    for para in doc.sections.iter_mut().flat_map(|s| &mut s.paragraphs) {
        for mark in &mut para.markpen_marks {
            if mark.color.is_some() {
                mark.color = Some("#123456".into());
            }
        }
    }
    let hwp = rhwp::serializer::serialize_document(&doc).expect("HWP5 저장");
    let reloaded = rhwp::parser::parse_document(&hwp).expect("HWP5 재적재");
    let ranges: Vec<_> = reloaded
        .sections
        .iter()
        .flat_map(|s| &s.paragraphs)
        .flat_map(|p| &p.range_tags)
        .filter(|r| r.tag >> 24 == 2)
        .collect();
    assert_eq!(ranges.len(), 3);
    assert!(
        ranges.iter().all(|r| r.tag == 0x0256_3412),
        "한컴 실측 COLORREF"
    );
    assert!(
        ranges.iter().any(|r| r.start == 0 && r.end == 8),
        "표 슬롯 전체"
    );
    assert_marks(&reloaded, "#123456");
}

// ── 구역 정의·하이퍼링크 뒤 형광펜 ────────────────────────────────────────
//
// 문단 첫머리 구역·단 정의(16유닛)와 하이퍼링크 필드(앞뒤 8유닛씩)가 형광펜 앞에 있다.
// "앞 링크 형광펜 뒤 글자들": 형광펜은 글자 5..8, HWP5 축 37..40 이다.

const LINKED_RUN: &str = concat!(
    r#"<hp:run charPrIDRef="0"><hp:t>앞 </hp:t><hp:ctrl><hp:fieldBegin id="1" type="HYPERLINK" name="" editable="0" dirty="0">"#,
    r#"<hp:parameters cnt="1" name=""><hp:stringParam name="Command">https\://a.example;1;0;0;</hp:stringParam></hp:parameters>"#,
    r#"</hp:fieldBegin></hp:ctrl><hp:t>링크</hp:t><hp:ctrl><hp:fieldEnd beginIDRef="1"/></hp:ctrl>"#,
    r##"<hp:t> <hp:markpenBegin color="#FFFF00"/>형광펜<hp:markpenEnd/> 뒤 글자들</hp:t></hp:run>"##,
);
/// (시작, 끝, 태그). 한컴 실측 COLORREF: `#FFFF00` → BGR `0x00FFFF`, 종류 2.
const YELLOW_MARKPEN: (u32, u32, u32) = (37, 40, 0x0200_FFFF);

fn hwpx_with_markpen_after_link() -> Vec<u8> {
    let template = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/samples/hwpx/blank_hwpx.hwpx"
    ))
    .expect("빈 HWPX");
    let mut src = zip::ZipArchive::new(Cursor::new(template)).expect("ZIP");
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..src.len() {
        let mut entry = src.by_index(index).unwrap();
        let name = entry.name().expect("엔트리 이름").into_owned();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        if name == "Contents/section0.xml" {
            let xml = String::from_utf8(data).unwrap();
            let empty = r#"<hp:run charPrIDRef="0"><hp:t/></hp:run>"#;
            assert_eq!(xml.matches(empty).count(), 1, "빈 런 하나");
            data = xml.replace(empty, LINKED_RUN).into_bytes();
        }
        let method = if name == "mimetype" {
            zip::CompressionMethod::Stored
        } else {
            zip::CompressionMethod::Deflated
        };
        out.start_file(
            name.as_str(),
            zip::write::SimpleFileOptions::default().compression_method(method),
        )
        .unwrap();
        out.write_all(&data).unwrap();
    }
    out.finish().unwrap().into_inner()
}

fn markpen_tags(core: &DocumentCore) -> Vec<(u32, u32, u32)> {
    let para = &core.document().sections[0].paragraphs[0];
    assert_eq!(para.text, "앞 링크 형광펜 뒤 글자들");
    para.range_tags
        .iter()
        .filter(|r| r.tag >> 24 == 2)
        .map(|r| (r.start, r.end, r.tag))
        .collect()
}

fn markpen_rects(node: &RenderNode, out: &mut Vec<(f64, f64)>) {
    if let RenderNodeType::Rectangle(rect) = &node.node_type {
        if node.editor_only && rect.style.fill_color == Some(0x0000_FFFF) {
            out.push((node.bbox.x, node.bbox.x + node.bbox.width));
        }
    }
    for child in &node.children {
        markpen_rects(child, out);
    }
}

/// 형광펜 사각형 하나가 글자 5(형)부터 8(펜 다음)까지의 캐럿 x 를 덮는다.
fn assert_drawn_on_markpen_text(core: &DocumentCore) {
    let caret_x = |offset| {
        let json: serde_json::Value =
            serde_json::from_str(&core.get_cursor_rect_native(0, 0, offset).unwrap()).unwrap();
        json["x"].as_f64().unwrap()
    };
    let (left, right) = (caret_x(5), caret_x(8));
    let mut rects = Vec::new();
    markpen_rects(&core.build_page_render_tree(0).unwrap().root, &mut rects);
    assert_eq!(rects.len(), 1, "형광펜 사각형 하나: {rects:?}");
    let (x0, x1) = rects[0];
    assert!(
        (x0 - left).abs() < 1.0 && (x1 - right).abs() < 1.0,
        "형광펜 {x0:.1}..{x1:.1} 이 '형광펜' {left:.1}..{right:.1} 에 그려져야 한다"
    );
}

#[test]
fn hwpx_markpen_after_controls_is_drawn_on_its_characters() {
    let core = DocumentCore::from_bytes(&hwpx_with_markpen_after_link()).unwrap();
    assert_eq!(markpen_tags(&core), vec![YELLOW_MARKPEN]);
    assert_drawn_on_markpen_text(&core);
}

#[test]
fn hwpx_markpen_after_link_keeps_its_range_through_hwp() {
    let source = DocumentCore::from_bytes(&hwpx_with_markpen_after_link()).unwrap();
    let hwp = source.export_hwp_with_adapter_snapshot().unwrap();
    let reopened = DocumentCore::from_bytes(&hwp).unwrap();
    assert_eq!(markpen_tags(&reopened), vec![YELLOW_MARKPEN]);
    assert_drawn_on_markpen_text(&reopened);

    // HWP 에서 다시 HWPX 로 저장해도 같은 글자를 감싼다.
    let hwpx = rhwp::parser::parse_document(&reopened.export_hwpx_native().unwrap()).unwrap();
    let marks: Vec<_> = hwpx.sections[0].paragraphs[0]
        .markpen_marks
        .iter()
        .map(|m| (m.char_idx, m.color.clone()))
        .collect();
    assert_eq!(marks, vec![(5, Some("#FFFF00".into())), (8, None)]);
}

#[test]
fn inserting_a_link_before_markpen_moves_the_saved_marks() {
    let mut core = DocumentCore::from_bytes(&hwpx_with_markpen_after_link()).unwrap();
    core.insert_hyperlink_native(&HyperlinkTarget::body(0, 0), 0, 1, "https://b.example")
        .unwrap();
    let moved = (53, 56, YELLOW_MARKPEN.2);
    assert_eq!(markpen_tags(&core), vec![moved]);
    assert_drawn_on_markpen_text(&core);
    let hwp = core.export_hwp_with_adapter_snapshot().unwrap();
    assert_eq!(
        markpen_tags(&DocumentCore::from_bytes(&hwp).unwrap()),
        vec![moved]
    );
}
