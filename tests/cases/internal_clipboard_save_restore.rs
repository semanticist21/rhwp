//! 편집이 내부 클립보드를 작업 자리로 빌린 뒤 사용자의 마지막 복사를 되돌린다.
//! 앱의 끌어 옮기기·문단 정렬·셀 정렬처럼 복사·붙이기로 문서를 고치는 편집 앞뒤에서 부른다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::wasm_api::HwpDocument;

fn doc_with(text: &str) -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document().unwrap();
    doc.insert_text_native(0, 0, 0, text).unwrap();
    doc
}

#[test]
fn borrowed_clipboard_returns_the_last_copy() {
    let mut doc = doc_with("사용자 작업");
    doc.copy_selection_native(0, 0, 0, 0, 3).unwrap();
    assert_eq!(doc.get_clipboard_text_native(), "사용자");

    doc.save_internal_clipboard_native();
    assert!(
        !doc.has_internal_clipboard_native(),
        "빌린 동안은 비어 있다"
    );
    doc.copy_selection_native(0, 0, 4, 0, 6).unwrap();
    assert_eq!(doc.get_clipboard_text_native(), "작업");
    // 겹쳐 빌려도 차례대로 돌아온다.
    doc.save_internal_clipboard_native();
    doc.clear_clipboard_native();
    assert!(doc.restore_internal_clipboard_native());
    assert_eq!(doc.get_clipboard_text_native(), "작업");
    assert!(doc.restore_internal_clipboard_native());
    assert_eq!(doc.get_clipboard_text_native(), "사용자");
    assert!(
        !doc.restore_internal_clipboard_native(),
        "비켜 둔 것이 없다"
    );

    // 되돌린 조각을 그대로 붙인다.
    doc.paste_internal_native(0, 0, 0).unwrap();
    assert_eq!(
        doc.get_text_range_native(0, 0, 0, 3).unwrap(),
        "사용자",
        "되돌린 조각을 붙인다"
    );
}

#[test]
fn restoring_brings_back_an_empty_clipboard() {
    let mut doc = doc_with("작업");
    doc.save_internal_clipboard_native();
    doc.copy_selection_native(0, 0, 0, 0, 2).unwrap();
    assert!(doc.restore_internal_clipboard_native());
    assert!(
        !doc.has_internal_clipboard_native(),
        "빌리기 전처럼 비어 있다"
    );
}

#[test]
fn borrowing_leaves_the_transpose_buffer_alone() {
    let mut doc = doc_with("표");
    let created: serde_json::Value =
        serde_json::from_str(&doc.create_table_native(0, 0, 1, 2, 2).unwrap()).unwrap();
    let para = created["paraIdx"].as_u64().unwrap() as usize;
    let control = created["controlIdx"].as_u64().unwrap_or(0) as usize;
    doc.insert_text_in_cell_native(0, para, control, 1, 0, 0, "b")
        .unwrap();
    doc.copy_table_cells_transposed_native(0, para, control, 0, 0, 1, 1)
        .unwrap();
    // 행/열 바꿈 버퍼는 그 명령만 쓴다. 빌리는 동안에도 남아 메뉴가 꺼지지 않는다.
    doc.save_internal_clipboard_native();
    assert!(doc.has_table_transpose_clipboard_native());
    assert!(doc.restore_internal_clipboard_native());
    assert!(doc.has_table_transpose_clipboard_native());
    doc.paste_table_cells_transposed_native(0, para, control, 0, 0)
        .unwrap();
}
