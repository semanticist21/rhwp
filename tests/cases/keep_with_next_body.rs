//! 다음 문단과 함께의 본문 쪽 배치와 저장 계약.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::model::paragraph::ColumnBreakType;
use serde_json::Value;

fn source() -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.begin_batch_native().unwrap();
    for index in 0..80 {
        if index > 0 {
            core.insert_paragraph_native(0, index).unwrap();
        }
        core.insert_text_native(0, index, 0, &format!("연결 문단 {index:02}"))
            .unwrap();
    }
    core.end_batch_native().unwrap();
    core
}

fn caret(core: &DocumentCore, paragraph: usize) -> Value {
    serde_json::from_str(&core.get_cursor_rect_native(0, paragraph, 0).unwrap()).unwrap()
}

fn boundary(core: &DocumentCore) -> usize {
    (1..80)
        .find(|&index| caret(core, index)["pageIndex"] != caret(core, 0)["pageIndex"])
        .unwrap()
}

fn keep(core: &mut DocumentCore, paragraph: usize) {
    core.apply_para_format_native(0, paragraph, r#"{"keepWithNext":true}"#)
        .unwrap();
}

fn assert_text_and_pages(core: &DocumentCore) {
    for (index, paragraph) in core.document().sections[0].paragraphs.iter().enumerate() {
        assert_eq!(paragraph.text, format!("연결 문단 {index:02}"));
    }
    for page in 0..core.page_count() {
        let text = core.page_text(page as usize).unwrap();
        assert!(!text.trim().is_empty(), "불필요한 빈 쪽 {page}");
    }
}

#[test]
fn moves_boundary_pair_and_preserves_caret_hit_and_roundtrips() {
    let mut core = source();
    let next = boundary(&core);
    keep(&mut core, next - 1);
    assert_eq!(
        caret(&core, next - 1)["pageIndex"],
        caret(&core, next)["pageIndex"]
    );
    assert_eq!(core.page_count(), 2);
    assert_text_and_pages(&core);
    for paragraph in [next - 1, next] {
        let rect = caret(&core, paragraph);
        let hit: Value = serde_json::from_str(
            &core
                .hit_test_native(
                    rect["pageIndex"].as_u64().unwrap() as u32,
                    rect["x"].as_f64().unwrap() + 1.0,
                    rect["y"].as_f64().unwrap() + rect["height"].as_f64().unwrap() / 2.0,
                )
                .unwrap(),
        )
        .unwrap();
        assert_eq!(hit["paragraphIndex"], paragraph);
    }
    for bytes in [
        core.export_hwp_native().unwrap(),
        core.export_hwpx_native().unwrap(),
    ] {
        let reopened = DocumentCore::from_bytes(&bytes).unwrap();
        assert_eq!(
            caret(&reopened, next - 1)["pageIndex"],
            caret(&reopened, next)["pageIndex"]
        );
        assert_text_and_pages(&reopened);
    }
}

#[test]
fn long_successor_keeps_first_line_and_splits_the_rest() {
    let mut core = source();
    let next = boundary(&core);
    let text = "긴 본문 ".repeat(2000);
    core.insert_text_native(0, next, 0, &text).unwrap();
    keep(&mut core, next - 1);
    for bytes in [
        core.export_hwp_native().unwrap(),
        core.export_hwpx_native().unwrap(),
    ] {
        let reopened = DocumentCore::from_bytes(&bytes).unwrap();
        assert_eq!(
            caret(&reopened, next - 1)["pageIndex"],
            caret(&reopened, next)["pageIndex"]
        );
        let end: Value = serde_json::from_str(
            &reopened
                .get_cursor_rect_native(0, next, text.chars().count())
                .unwrap(),
        )
        .unwrap();
        assert!(end["pageIndex"].as_u64() > caret(&reopened, next)["pageIndex"].as_u64());
        assert_eq!(
            reopened.document().sections[0].paragraphs[next].text,
            format!("{text}연결 문단 {next:02}")
        );
        for page in 0..reopened.page_count() {
            assert!(!reopened.page_text(page as usize).unwrap().trim().is_empty());
        }
    }
    assert_eq!(
        caret(&core, next - 1)["pageIndex"],
        caret(&core, next)["pageIndex"]
    );
}

#[test]
fn consecutive_links_move_together_and_last_link_is_noop() {
    let mut core = source();
    let next = boundary(&core);
    core.begin_batch_native().unwrap();
    for paragraph in next - 3..next {
        keep(&mut core, paragraph);
    }
    core.end_batch_native().unwrap();
    for paragraph in next - 3..next {
        assert_eq!(
            caret(&core, paragraph)["pageIndex"],
            caret(&core, next)["pageIndex"]
        );
    }
    let before = caret(&core, 79);
    keep(&mut core, 79);
    assert_eq!(caret(&core, 79), before);
    assert_text_and_pages(&core);
}

#[test]
fn oversized_chain_terminates_without_empty_pages() {
    let mut core = source();
    core.begin_batch_native().unwrap();
    for paragraph in 0..80 {
        keep(&mut core, paragraph);
    }
    core.end_batch_native().unwrap();
    assert!(core.page_count() <= 3);
    assert_text_and_pages(&core);
}

#[test]
fn explicit_boundaries_override_links() {
    for kind in [
        ColumnBreakType::Page,
        ColumnBreakType::Column,
        ColumnBreakType::Section,
    ] {
        let mut core = source();
        let next = boundary(&core);
        core.document_mut().sections[0].paragraphs[next].column_type = kind;
        core.set_document(core.document().clone());
        keep(&mut core, next - 1);
        assert_ne!(
            caret(&core, next - 1)["pageIndex"],
            caret(&core, next)["pageIndex"],
            "{kind:?}"
        );
        assert_text_and_pages(&core);
    }
    let mut core = source();
    let next = boundary(&core);
    core.apply_para_format_native(0, next, r#"{"pageBreakBefore":true}"#)
        .unwrap();
    keep(&mut core, next - 1);
    assert_ne!(
        caret(&core, next - 1)["pageIndex"],
        caret(&core, next)["pageIndex"]
    );
    assert_text_and_pages(&core);
}

#[test]
fn enabling_keep_after_reopen_moves_a_stored_boundary() {
    let original = source();
    for bytes in [
        original.export_hwp_native().unwrap(),
        original.export_hwpx_native().unwrap(),
    ] {
        let mut core = DocumentCore::from_bytes(&bytes).unwrap();
        let next = boundary(&core);
        let before = caret(&core, next - 1);
        keep(&mut core, next - 1);
        assert_eq!(
            caret(&core, next - 1)["pageIndex"],
            caret(&core, next)["pageIndex"]
        );
        assert_ne!(caret(&core, next - 1)["pageIndex"], before["pageIndex"]);
        assert_eq!(core.page_count(), 2);
        assert_text_and_pages(&core);
    }
}
