//! #6963 후속: 문서 내부 책갈피 링크의 Command 인코딩·삽입·수정·조회·저장 왕복.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::hyperlink::HyperlinkTarget;
use rhwp::document_core::hyperlink::LinkObjectAddress;
use rhwp::document_core::DocumentCore;
use rhwp::model::hyperlink::{
    bookmark_command, command_bookmark, file_link, file_link_command, hwp_link, hwp_link_command,
    HwpLink,
};

const NAME: &str = r"대상:가;나\다";

/// 0번 문단은 링크를 걸 글, 1번 문단은 책갈피 두 개를 담는다.
fn doc_with_bookmarks() -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.insert_text_native(0, 0, 0, "링크 문단책갈피 문단")
        .unwrap();
    core.split_paragraph_native(0, 0, 5, None).unwrap();
    for (offset, name) in [(0, NAME), (3, "둘째")] {
        let result = core.add_bookmark_native(0, 1, offset, name).unwrap();
        assert!(result.contains("\"ok\":true"), "{result}");
    }
    core
}

fn roundtrips(core: &DocumentCore) -> Vec<DocumentCore> {
    [
        core.export_hwp_native().unwrap(),
        core.export_hwpx_native().unwrap(),
    ]
    .iter()
    .map(|bytes| DocumentCore::from_bytes(bytes).unwrap())
    .collect()
}

fn only_link(core: &DocumentCore) -> (Option<String>, String, usize, usize) {
    let links = core
        .hyperlinks_native(&HyperlinkTarget::body(0, 0))
        .unwrap();
    assert_eq!(links.len(), 1, "{links:?}");
    let l = &links[0];
    (l.bookmark.clone(), l.uri.clone(), l.start, l.end)
}

#[test]
fn bookmark_command_matches_hancom_encoding() {
    // 한컴 샘플의 실제 Command와 같은 문자열이어야 한다.
    assert_eq!(
        bookmark_command("예제 : 배포용 문서 만들기").unwrap(),
        r"?예제 \: 배포용 문서 만들기;0;0;0;"
    );
    let command = bookmark_command(NAME).unwrap();
    assert_eq!(command, r"?대상\:가\;나\\다;0;0;0;");
    assert_eq!(command_bookmark(&command).as_deref(), Some(NAME));
    assert_eq!(command_bookmark(r"https\://a.example;1;0;0;"), None);
    for invalid in ["", "  ", "줄\n바꿈"] {
        assert!(bookmark_command(invalid).is_err(), "{invalid:?}");
    }
}

#[test]
fn hancom_sample_bookmark_link_reports_its_bookmark() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples/hwpctl_Action_Table__v1.1.hwp");
    let mut core = DocumentCore::from_bytes(&std::fs::read(path).unwrap()).unwrap();
    let cell = HyperlinkTarget {
        section: 0,
        para: 8,
        cell_path: vec![(0, 200, 2)],
    };
    let links = core.hyperlinks_native(&cell).unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].text, "배포용 문서 만들기");
    assert_eq!(
        links[0].bookmark.as_deref(),
        Some("예제 : 배포용 문서 만들기")
    );
    // 대상은 블록 책갈피 필드다. 같은 대상으로 링크를 수정하면 변경 없음이다.
    let id = links[0].field_id;
    assert!(!core
        .update_bookmark_hyperlink_native(&cell, id, "예제 : 배포용 문서 만들기")
        .unwrap());
}

#[test]
fn bookmark_link_inserts_updates_and_survives_hwp_and_hwpx() {
    let mut core = doc_with_bookmarks();
    let target = HyperlinkTarget::body(0, 0);
    assert!(core
        .insert_bookmark_hyperlink_native(&target, 0, 2, "없는 책갈피")
        .is_err());
    let id = core
        .insert_bookmark_hyperlink_native(&target, 0, 2, NAME)
        .unwrap();
    for reopened in roundtrips(&core) {
        assert_eq!(
            only_link(&reopened),
            (Some(NAME.to_string()), format!("?{NAME}"), 0, 2)
        );
    }
    // 웹 주소로 바꾸거나 없는 책갈피로 바꾸지 않는다.
    assert!(core
        .update_hyperlink_native(&target, id, "https://example.com")
        .is_err());
    assert!(core
        .update_bookmark_hyperlink_native(&target, id, "없는 책갈피")
        .is_err());
    assert!(!core
        .update_bookmark_hyperlink_native(&target, id, NAME)
        .unwrap());
    assert!(core
        .update_bookmark_hyperlink_native(&target, id, "둘째")
        .unwrap());
    for reopened in roundtrips(&core) {
        assert_eq!(
            only_link(&reopened),
            (Some("둘째".to_string()), "?둘째".to_string(), 0, 2)
        );
    }
}

#[test]
fn web_link_is_not_turned_into_bookmark_link() {
    let mut core = doc_with_bookmarks();
    let target = HyperlinkTarget::body(0, 0);
    let id = core
        .insert_hyperlink_native(&target, 0, 2, "https://example.com")
        .unwrap();
    assert!(core
        .update_bookmark_hyperlink_native(&target, id, "둘째")
        .is_err());
    assert_eq!(only_link(&core).0, None);
}

#[test]
fn hancom_document_and_file_link_commands() {
    // 한컴 12.30 Mac이 저장한 Command(2026-11-16): 문서의 처음·개요·다른 문서·외부 파일.
    assert_eq!(hwp_link(";0;0;0;"), Some(HwpLink::default()));
    let outline = hwp_link("?#171526264;0;1;0;").unwrap();
    assert_eq!(
        (outline.anchor.as_str(), outline.target_type),
        ("#171526264", 1)
    );
    assert_eq!(command_bookmark("?#171526264;0;1;0;"), None);
    assert_eq!(command_bookmark(";0;0;0;"), None);
    let other = hwp_link("/Users/a/other.hwpx?#174334617;0;1;2;").unwrap();
    assert_eq!(
        other,
        HwpLink {
            path: "/Users/a/other.hwpx".into(),
            anchor: "#174334617".into(),
            target_type: 1,
            open: 2,
        }
    );
    let other_bookmark = HwpLink {
        path: "/Users/a/other.hwpx".into(),
        anchor: "책갈피하나".into(),
        ..Default::default()
    };
    for (link, command) in [
        (HwpLink::default(), ";0;0;0;"),
        (outline, "?#171526264;0;1;0;"),
        (other, "/Users/a/other.hwpx?#174334617;0;1;2;"),
        (
            other_bookmark.clone(),
            "/Users/a/other.hwpx?책갈피하나;0;0;0;",
        ),
    ] {
        assert_eq!(hwp_link_command(&link).unwrap(), command);
        assert_eq!(hwp_link(command), Some(link));
    }
    assert_eq!(
        file_link("./note.txt;3;0;0;").as_deref(),
        Some("./note.txt")
    );
    assert_eq!(
        file_link_command("./note.txt").unwrap(),
        "./note.txt;3;0;0;"
    );
    assert_eq!(hwp_link(r"https\://a.example;1;0;0;"), None);
    assert_eq!(file_link(r"https\://a.example;1;0;0;"), None);
    // 경로의 `?`는 대상 구분자와 헷갈리지 않게 이스케이프한다.
    let odd = HwpLink {
        path: "/a?b;c.hwp".into(),
        ..other_bookmark
    };
    assert_eq!(hwp_link(&hwp_link_command(&odd).unwrap()), Some(odd));
}

#[test]
fn document_links_reach_outline_table_and_equation_after_save() {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.insert_text_native(0, 0, 0, "개요 문단가 나 다 라 마")
        .unwrap();
    core.split_paragraph_native(0, 0, 5, None).unwrap();
    core.split_paragraph_native(0, 1, 9, None).unwrap();
    core.split_paragraph_native(0, 2, 0, None).unwrap();
    // 2번은 한컴 기본 스타일의 개요 1이다.
    core.apply_style_native(0, 0, 2).unwrap();
    core.create_table_native(0, 2, 0, 2, 2).unwrap();
    let last = core.document().sections[0].paragraphs.len() - 1;
    core.insert_equation_native(0, last, 0, "a+b", 1000, 0)
        .unwrap();
    let dest = core.link_destinations_native();
    assert_eq!(dest["outlines"].as_array().unwrap().len(), 1, "{dest}");
    let address = |item: &serde_json::Value| -> LinkObjectAddress {
        serde_json::from_value(serde_json::json!({
            "section": item["section"],
            "para": item["para"],
            "cellPath": item["cellPath"],
            "control": item.get("control"),
        }))
        .unwrap()
    };
    let target = HyperlinkTarget::body(0, 1);
    let link = |target_type| HwpLink {
        target_type,
        ..Default::default()
    };
    core.insert_hwp_hyperlink_native(&target, 0, 1, &link(0), None)
        .unwrap();
    for (offset, key, kind) in [(2, "outlines", 1), (4, "tables", 2), (6, "equations", 4)] {
        let at = address(&dest[key][0]);
        // 대상 종류와 자리가 맞지 않으면 거절한다.
        assert!(core
            .insert_hwp_hyperlink_native(&target, offset, offset + 1, &link(3), Some(&at))
            .is_err());
        core.insert_hwp_hyperlink_native(&target, offset, offset + 1, &link(kind), Some(&at))
            .unwrap();
    }
    core.insert_file_hyperlink_native(&target, 8, 9, "./note.txt")
        .unwrap();
    for reopened in roundtrips(&core) {
        let links = reopened.hyperlinks_native(&target).unwrap();
        assert_eq!(links.len(), 5, "{links:?}");
        assert_eq!(links[0].hwp, Some(HwpLink::default()));
        assert_eq!(links[0].bookmark, None);
        assert_eq!(links[4].file.as_deref(), Some("./note.txt"));
        let dest = reopened.link_destinations_native();
        for (link, key) in links[1..4].iter().zip(["outlines", "tables", "equations"]) {
            let hwp = link.hwp.as_ref().unwrap();
            assert_eq!(link.bookmark, None);
            let id = dest[key][0]["id"].as_u64().unwrap();
            assert!(id > 0);
            assert_eq!(hwp.anchor, format!("#{id}"), "{key} {dest}");
        }
    }
}
