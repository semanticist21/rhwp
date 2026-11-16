//! 문서 정보 창 API — 문서 요약 읽기·쓰기, 그림 정보, 저작권, 개체 수.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::queries::document_info::DocumentSummary;
use rhwp::wasm_api::HwpDocument;

fn open(name: &str) -> HwpDocument {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join(name);
    HwpDocument::from_bytes(&std::fs::read(path).unwrap()).unwrap()
}

fn patch() -> DocumentSummary {
    DocumentSummary {
        title: Some("새 제목 & <부제>".into()),
        subject: Some("주제".into()),
        author: Some("지은이".into()),
        date: Some("2026년 11월 15일 일요일 오후 9:36:59".into()),
        keywords: Some("가, 나".into()),
        comments: Some("첫 줄\n둘째 줄".into()),
        last_saved_by: Some("kkom".into()),
        created_at: Some("2004-09-15T12:15:12Z".into()),
        last_saved_at: Some("2026-11-15T12:46:54Z".into()),
    }
}

/// HWP 로 연 문서와 HWPX 로 연 문서 모두, 고친 요약이 HWP·HWPX 어느 쪽으로 저장해도 남는다.
#[test]
fn summary_survives_hwp_and_hwpx_saves_from_either_source() {
    for name in ["field-01.hwp", "pic2.hwpx"] {
        let mut doc = open(name);
        let before = doc.document_summary();
        assert!(before.title.is_some(), "{name}: 읽으면 모든 글 칸을 채운다");
        doc.update_document_summary(patch());
        assert_eq!(doc.document_summary(), patch(), "{name}");
        for bytes in [doc.export_hwp().unwrap(), doc.export_hwpx().unwrap()] {
            let reopened = HwpDocument::from_bytes(&bytes).unwrap();
            assert_eq!(reopened.document_summary(), patch(), "{name}");
        }
    }
}

/// 일부 칸만 고치면 나머지 칸은 그대로다(저장할 때 마지막 저장한 사람·날짜만 바꾸는 경우).
#[test]
fn partial_summary_patch_keeps_other_fields() {
    let mut doc = open("field-01.hwp");
    let before = doc.document_summary();
    doc.set_document_summary_json(
        r#"{"lastSavedBy":"새 사람","lastSavedAt":"2026-11-15T00:00:00Z"}"#,
    )
    .unwrap();
    let after = doc.document_summary();
    assert_eq!(after.last_saved_by.as_deref(), Some("새 사람"));
    assert_eq!(after.last_saved_at.as_deref(), Some("2026-11-15T00:00:00Z"));
    assert_eq!(after.title, before.title);
    assert_eq!(after.author, before.author);
    assert_eq!(after.created_at, before.created_at);
    assert!(doc
        .set_document_summary_json(r#"{"createdAt":"어제"}"#)
        .is_err());
    // 빈 날짜는 지운다(새 문서가 서식 파일의 저장 정보를 물려받지 않게).
    doc.set_document_summary_json(r#"{"lastSavedAt":"","lastSavedBy":""}"#)
        .unwrap();
    for bytes in [doc.export_hwp().unwrap(), doc.export_hwpx().unwrap()] {
        let reopened = HwpDocument::from_bytes(&bytes).unwrap().document_summary();
        assert_eq!(reopened.last_saved_at, None);
        assert_eq!(reopened.last_saved_by.as_deref(), Some(""));
        assert_eq!(reopened.created_at, before.created_at);
    }
}

/// 머리말 안 그림도 표·그림·글상자 수와 그림 목록에 든다.
#[test]
fn header_pictures_are_counted_and_listed_with_their_page() {
    let doc = open("pic-in-head-01.hwp");
    let count = doc.object_count();
    assert!(count.pictures >= 1, "{count:?}");
    let controls: Vec<serde_json::Value> = serde_json::from_str(&doc.get_controls()).unwrap();
    let body_pictures = controls.iter().filter(|c| c["userDesc"] == "그림").count() as u32;
    assert!(
        count.pictures > body_pictures,
        "{count:?} 본문 {body_pictures}"
    );
    let list = doc.bin_data_infos();
    assert!(!list.is_empty());
    assert!(list.iter().all(|b| b.kind == "embed" && b.path.is_empty()));
    assert_eq!(list[0].id, 1);
    assert_eq!(
        list.iter()
            .find(|b| b.first_page.is_some())
            .and_then(|b| b.first_page),
        Some(1)
    );
}

/// 저작권 — HWPX `hh:licensemark`, HWP FileHeader 의 CCL 비트·조건·언어(한컴 12.30 저장본에서 잰 자리).
#[test]
fn license_mark_reads_hwpx_and_hwp_storage() {
    let mut doc = open("pic2.hwpx");
    assert_eq!(doc.license_mark(), None);
    doc.document_mut().doc_info.hwpx_head_tail = Some(
        r#"<hh:docOption><hh:linkinfo path="" pageInherit="0" footnoteInherit="0"/><hh:licensemark type="CCL" flag="5" lang="6"/></hh:docOption>"#
            .into(),
    );
    assert_eq!(doc.license_mark(), Some(("CCL".into(), 5, 6)));
    let json: serde_json::Value = serde_json::from_str(&doc.license_mark_json()).unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "type": "CCL", "flag": 5, "lang": 6 })
    );

    let mut doc = open("field-01.hwp");
    assert_eq!(doc.license_mark(), None);
    let raw = doc.document_mut().header.raw_data.as_mut().unwrap();
    raw[36..40].copy_from_slice(&(0x801u32).to_le_bytes());
    raw[40..44].copy_from_slice(&2u32.to_le_bytes());
    raw[48..52].copy_from_slice(&6u32.to_le_bytes());
    assert_eq!(doc.license_mark(), Some(("CCL".into(), 2, 6)));
}

/// CCL 넣기 — HWP·HWPX 원본 모두 적은 CCL 이 HWP·HWPX 어느 쪽으로 저장해도 한컴 자리에 남는다.
#[test]
fn ccl_license_mark_is_written_to_both_formats() {
    for name in ["field-01.hwp", "pic2.hwpx"] {
        let mut doc = open(name);
        doc.set_ccl_license_mark(5).unwrap();
        assert_eq!(doc.license_mark(), Some(("CCL".into(), 5, 6)), "{name}");
        // 한컴처럼 이미 있으면 새 조건으로 바꾼다.
        doc.set_ccl_license_mark(2).unwrap();
        assert!(
            doc.set_ccl_license_mark(6).is_err(),
            "변경 불허와 동일 조건은 함께 못 쓴다"
        );
        assert!(doc.set_ccl_license_mark(8).is_err());

        let hwp = HwpDocument::from_bytes(&doc.export_hwp().unwrap()).unwrap();
        let raw = hwp.document().header.raw_data.clone().unwrap();
        let at = |off: usize| u32::from_le_bytes(raw[off..off + 4].try_into().unwrap());
        assert_ne!(at(36) & (1 << 11), 0, "{name}: FileHeader 속성 bit 11");
        assert_eq!((at(40), at(48)), (2, 6), "{name}");
        assert_eq!(hwp.license_mark(), Some(("CCL".into(), 2, 6)), "{name}");

        let hwpx = HwpDocument::from_bytes(&doc.export_hwpx().unwrap()).unwrap();
        let tail = hwpx.document().doc_info.hwpx_head_tail.clone().unwrap();
        assert_eq!(tail.matches("<hh:licensemark").count(), 1, "{name}: {tail}");
        assert!(
            tail.contains(r#"<hh:licensemark type="CCL" flag="2" lang="6"/></hh:docOption>"#),
            "{name}: {tail}"
        );
    }
}

fn first_picture(doc: &mut HwpDocument) -> &mut rhwp::model::image::Picture {
    doc.document_mut().sections[0]
        .paragraphs
        .iter_mut()
        .flat_map(|p| p.controls.iter_mut())
        .find_map(|c| match c {
            rhwp::model::control::Control::Picture(p) => Some(p.as_mut()),
            _ => None,
        })
        .expect("그림")
}

/// HWP 그림 하이퍼링크(CTRL_DATA)를 `href` 로 읽어, HWP 를 HWPX 로 저장해도 그림 링크가 남는다.
#[test]
fn hwp_picture_link_is_read_as_href() {
    let url = "http://creativecommons.org/licenses/by/4.0/deed.ko;1;0;1";
    let mut doc = open("pic2.hwpx");
    first_picture(&mut doc).href = Some(url.into());
    let mut hwp = HwpDocument::from_bytes(&doc.export_hwp().unwrap()).unwrap();
    assert_eq!(first_picture(&mut hwp).href.as_deref(), Some(url));
    let mut hwpx = HwpDocument::from_bytes(&hwp.export_hwpx().unwrap()).unwrap();
    assert_eq!(first_picture(&mut hwpx).href.as_deref(), Some(url));
}

/// 상용구의 현재 쪽 번호·전체 쪽수 — 글 가운데 넣은 자동 번호가 HWP·HWPX 저장 뒤에도 그 자리에 있다.
#[test]
fn auto_number_inserted_mid_text_keeps_its_place() {
    use rhwp::model::control::{AutoNumberType, Control};
    let mut doc = open("field-01.hwp");
    let para = doc.document().sections[0]
        .paragraphs
        .iter()
        .position(|p| p.controls.is_empty() && p.text.chars().count() >= 2)
        .expect("글만 든 문단");
    let text = doc.document().sections[0].paragraphs[para].text.clone();
    doc.insert_auto_number_at(0, para, 1, AutoNumberType::Page)
        .unwrap();
    doc.insert_auto_number_at(0, para, 3, AutoNumberType::TotalPage)
        .unwrap();
    for bytes in [doc.export_hwp().unwrap(), doc.export_hwpx().unwrap()] {
        let reopened = HwpDocument::from_bytes(&bytes).unwrap();
        let p = &reopened.document().sections[0].paragraphs[para];
        let kinds: Vec<_> = p
            .controls
            .iter()
            .filter_map(|c| match c {
                Control::AutoNumber(a) => Some(a.number_type),
                _ => None,
            })
            .collect();
        assert_eq!(kinds, [AutoNumberType::Page, AutoNumberType::TotalPage]);
        // 자리표 글자 둘이 첫 글자 뒤와 (그 뒤) 둘째 글자 뒤에 있다.
        let mut expected: Vec<char> = text.chars().collect();
        expected.insert(1, ' ');
        expected.insert(3, ' ');
        assert_eq!(p.text, expected.into_iter().collect::<String>());
    }
}
