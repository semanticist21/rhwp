//! [#8017] HWP5 memo lists survive a save that rebuilds the section from the model.

use rhwp::model::control::{Control, Field};
use rhwp::model::document::Document;
use rhwp::model::paragraph::Paragraph;
use rhwp::parser::parse_document;
use rhwp::serializer::serialize_document;

fn sample(rel: &str) -> Vec<u8> {
    let path = format!("{}/samples/{rel}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

fn collect_memos<'a>(paragraphs: &'a [Paragraph], out: &mut Vec<&'a Field>) {
    for para in paragraphs {
        for ctrl in &para.controls {
            match ctrl {
                Control::Field(field) if field.command.starts_with("MEMO/") => out.push(field),
                Control::Table(table) => {
                    for cell in &table.cells {
                        collect_memos(&cell.paragraphs, out);
                    }
                }
                _ => {}
            }
        }
    }
}

fn memos(doc: &Document) -> Vec<&Field> {
    let mut out = Vec::new();
    for section in &doc.sections {
        collect_memos(&section.paragraphs, &mut out);
    }
    out
}

/// Save the way an edited document is saved: every section from the model.
fn rebuild(doc: &Document) -> Document {
    let mut edited = doc.clone();
    edited.doc_info.raw_stream_dirty = true;
    for section in &mut edited.sections {
        section.raw_stream = None;
    }
    let bytes = serialize_document(&edited).expect("write HWP");
    parse_document(&bytes).expect("read the written HWP")
}

/// Hancom memo body paragraphs carry their own PARA_HEADER tail (instance id and,
/// from 5.0.3.2, the change-tracking word). The memo list writer replaced every
/// tail shorter than 12 bytes with zeros, so a Hancom 10-byte tail grew and lost
/// its instance id.
#[test]
fn memo_body_paragraph_header_tails_survive_a_rebuilt_save() {
    for rel in ["basic/NewYear_s_Day.hwp", "basic/english.hwp"] {
        let original = parse_document(&sample(rel)).expect("parse the sample");
        let before: Vec<Vec<Vec<u8>>> = memos(&original)
            .iter()
            .map(|field| {
                field
                    .memo_paragraphs
                    .iter()
                    .map(|para| para.raw_header_extra.clone())
                    .collect()
            })
            .collect();
        assert!(
            before.iter().any(|tails| !tails.is_empty()),
            "{rel} has memo bodies"
        );

        let reread = rebuild(&original);
        let after: Vec<Vec<Vec<u8>>> = memos(&reread)
            .iter()
            .map(|field| {
                field
                    .memo_paragraphs
                    .iter()
                    .map(|para| para.raw_header_extra.clone())
                    .collect()
            })
            .collect();
        assert_eq!(after, before, "{rel}: memo paragraph header tails");
    }
}

/// A memo body without a header tail (a new memo) is written with the zero tail.
#[test]
fn a_new_memo_body_gets_the_zero_header_tail() {
    let mut core = rhwp::document_core::DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.insert_text_native(0, 0, 0, "anchor").unwrap();
    core.insert_memo_at(0, 0, 0, 6, "tester", "one\ntwo")
        .unwrap();
    let bytes = core.export_hwp_native().expect("write HWP");
    let reread = parse_document(&bytes).expect("read the written HWP");
    let found = memos(&reread);
    assert_eq!(found.len(), 1);
    let tails: Vec<&[u8]> = found[0]
        .memo_paragraphs
        .iter()
        .map(|para| para.raw_header_extra.as_slice())
        .collect();
    assert_eq!(tails.len(), 2);
    for tail in tails {
        assert_eq!(tail.len(), 12);
        assert!(tail[6..].iter().all(|byte| *byte == 0));
    }
}

/// A memo document must not grow by one empty paragraph per HWP save. Hancom puts
/// MEMO_LIST right after the real last body paragraph.
#[test]
fn memo_tail_resave_keeps_the_paragraph_count() {
    use rhwp::parser::body_text::parse_body_text_section;
    use rhwp::parser::record::Record;
    use rhwp::parser::tags;
    use rhwp::serializer::body_text::serialize_section;

    let doc = parse_document(&sample("issue5866/memo_field_hwp5.hwp"))
        .expect("parse the Hancom memo sample");
    let mut section = doc
        .sections
        .into_iter()
        .last()
        .expect("the sample has a section");
    let original = section.paragraphs.len();
    for _ in 0..2 {
        section.raw_stream = None;
        section.raw_provenance = None;
        let bytes = serialize_section(&section);
        let records = Record::read_all(&bytes).unwrap();
        let last_body = records
            .iter()
            .rposition(|r| r.tag_id == tags::HWPTAG_PARA_HEADER && r.level == 0)
            .unwrap();
        let char_count = u32::from_le_bytes(records[last_body].data[0..4].try_into().unwrap());
        assert_ne!(
            char_count & 0x8000_0000,
            0,
            "the real last paragraph keeps its last bit"
        );
        assert!(
            records[last_body + 1..]
                .iter()
                .any(|r| r.tag_id == tags::HWPTAG_MEMO_LIST && r.level == 1),
            "the memo list follows the last body paragraph"
        );
        section = parse_body_text_section(&bytes).unwrap();
        assert_eq!(
            section.paragraphs.len(),
            original,
            "no root paragraph is added"
        );
        let mut found = Vec::new();
        collect_memos(&section.paragraphs, &mut found);
        assert_eq!(
            found
                .iter()
                .filter(|field| !field.memo_paragraphs.is_empty())
                .count(),
            1,
            "the memo body is read back"
        );
    }
}

/// 한컴 12.30은 모든 구역의 메모 목록을 마지막 구역 끝에 쓴다(두 구역 문서를 HWP로 저장해 확인:
/// 첫 구역 메모와 둘째 구역 메모의 MEMO_LIST가 모두 Section1 끝에 있었다). 다시 열면 앞 구역 메모도
/// 그 목록에서 글을 찾는다.
#[test]
fn every_section_memo_list_is_written_after_the_last_section() {
    use rhwp::document_core::DocumentCore;
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.insert_text_native(0, 0, 0, "첫 구역 글").unwrap();
    core.split_section_native(0, 0, 5).unwrap();
    assert_eq!(core.document().sections.len(), 2);
    core.insert_text_native(1, 0, 0, "둘째 구역 글").unwrap();
    core.insert_memo_at(0, 0, 0, 1, "t", "첫 메모").unwrap();
    core.insert_memo_at(1, 0, 0, 2, "t", "둘째 메모").unwrap();

    let bytes = serialize_document(core.document()).expect("write HWP");
    let mut cfb = rhwp::parser::cfb_reader::CfbReader::open(&bytes).unwrap();
    let compressed = cfb.read_file_header().unwrap()[36] & 1 != 0;
    let tails: Vec<usize> = (0..2)
        .map(|index| {
            let stream = cfb
                .read_body_text_section(index, compressed, false)
                .unwrap();
            rhwp::parser::record::Record::read_all(&stream)
                .unwrap()
                .iter()
                .filter(|record| record.tag_id == rhwp::parser::tags::HWPTAG_MEMO_LIST)
                .count()
        })
        .collect();
    assert_eq!(tails, vec![0, 2]);

    let reopened = parse_document(&bytes).expect("read the written HWP");
    let texts: Vec<String> = memos(&reopened)
        .iter()
        .map(|field| {
            field
                .memo_paragraphs
                .iter()
                .map(|p| p.text.clone())
                .collect()
        })
        .collect();
    assert_eq!(texts, vec!["첫 메모".to_string(), "둘째 메모".to_string()]);
}

#[test]
fn removing_an_earlier_section_memo_drops_its_list_from_the_last_section() {
    use rhwp::document_core::DocumentCore;
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.insert_text_native(0, 0, 0, "첫 구역 글").unwrap();
    core.split_section_native(0, 0, 5).unwrap();
    core.insert_text_native(1, 0, 0, "둘째 구역 글").unwrap();
    core.insert_memo_at(0, 0, 0, 1, "t", "첫 메모").unwrap();
    core.insert_memo_at(1, 0, 0, 2, "t", "둘째 메모").unwrap();
    let saved = serialize_document(core.document()).expect("write HWP");

    // 원본 스트림을 지닌 채 다시 연 문서에서 앞 구역 메모를 지운다.
    let mut core = DocumentCore::from_bytes(&saved).unwrap();
    let list: serde_json::Value = serde_json::from_str(&core.get_memo_list_json()).unwrap();
    let first = list
        .as_array()
        .unwrap()
        .iter()
        .find(|memo| memo["text"] == "첫 메모")
        .expect("첫 메모")["id"]
        .as_u64()
        .unwrap() as u32;
    core.remove_memo(first).unwrap();

    let bytes = serialize_document(core.document()).expect("write HWP");
    let reopened = parse_document(&bytes).expect("read the written HWP");
    let texts: Vec<String> = memos(&reopened)
        .iter()
        .map(|field| {
            field
                .memo_paragraphs
                .iter()
                .map(|p| p.text.clone())
                .collect()
        })
        .collect();
    assert_eq!(texts, vec!["둘째 메모".to_string()]);
    // 마지막 구역 끝 원본 목록에는 남은 메모 하나의 목록만 있다.
    let lists = reopened.sections[1]
        .trailing_lists
        .as_ref()
        .map_or(0, |tail| {
            tail.records
                .iter()
                .filter(|raw| raw.level == 1 && raw.tag_id == rhwp::parser::tags::HWPTAG_MEMO_LIST)
                .count()
        });
    assert_eq!(lists, 1, "지운 메모의 목록이 남지 않는다");
}
