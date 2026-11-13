//! 관계형. 한컴 12.30 스타일 창의 한 줄 위로·아래로 이동은 스타일 자리를 바꾸고 그 스타일을 쓴 문단·글자가
//! 따라가게 한다(도움말 `format/style/style(move).htm`, 바탕글은 옮기지 못한다). 기준: 옮긴 뒤에도 문단·
//! 글자 스타일·다음 스타일이 같은 스타일을 가리키고 HWP·HWPX 저장 뒤에도 같다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::DocumentCore;
use rhwp::wasm_api::HwpDocument;

fn reopened(core: &DocumentCore) -> Vec<(&'static str, DocumentCore)> {
    [
        ("HWP", core.export_hwp_native().unwrap()),
        ("HWPX", core.export_hwpx_native().unwrap()),
    ]
    .into_iter()
    .map(|(name, bytes)| (name, DocumentCore::from_bytes(&bytes).unwrap()))
    .collect()
}

fn style_named(core: &DocumentCore, name: &str) -> usize {
    core.document()
        .doc_info
        .styles
        .iter()
        .position(|s| s.local_name == name)
        .unwrap_or_else(|| panic!("{name} 스타일 없음"))
}

/// 438: 본문을 한 줄 아래로 옮기면 개요 1과 자리를 바꾸고, 문단·다음 스타일·글자 스타일 참조가 스타일을 따라간다.
#[test]
fn moving_a_style_keeps_every_reference() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, "가나다라").unwrap();
    let body = style_named(&doc, "본문");
    let outline = style_named(&doc, "개요 1");
    assert_eq!((body, outline), (1, 2), "기본 문서의 스타일 차례");
    let emphasis = doc.create_style(r#"{"name":"강조","englishName":"Emphasis","type":1}"#) as u32;
    doc.apply_style_native(0, 0, body).unwrap();
    doc.set_char_style_range_by_path(0, 0, &[], 1, 3, Some(emphasis))
        .unwrap();
    // 개요 1의 다음 스타일을 본문으로 둔다.
    assert!(doc.update_style(outline as u32, &format!(r#"{{"nextStyleId":{body}}}"#)));

    assert!(!doc.move_style(0, 1), "바탕글은 옮기지 못한다");
    assert!(!doc.move_style(1, 0), "바탕글 자리로 옮기지 못한다");
    assert!(doc.move_style(1, 2));
    // 글자 스타일을 맨 앞(2번째 자리)으로 옮겨 그 사이 스타일이 모두 한 칸 밀리게 한다.
    assert!(doc.move_style(emphasis, 1));

    let check = |core: &DocumentCore, label: &str| {
        let styles = &core.document().doc_info.styles;
        let names: Vec<_> = styles
            .iter()
            .take(4)
            .map(|s| s.local_name.as_str())
            .collect();
        assert_eq!(names, ["바탕글", "강조", "개요 1", "본문"], "{label}");
        let para = &core.document().sections[0].paragraphs[0];
        assert_eq!(para.style_id, 3, "{label}: 문단 스타일");
        let chars: Vec<_> = (0..4).map(|i| para.char_style_id_at(i)).collect();
        assert_eq!(
            chars,
            [None, Some(1), Some(1), None],
            "{label}: 글자 스타일"
        );
        assert_eq!(styles[2].next_style_id, 3, "{label}: 개요 1의 다음 스타일");
    };
    check(&doc, "편집 직후");
    for (name, core) in reopened(&doc) {
        check(&core, name);
    }
}
