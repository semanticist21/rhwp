//! 크기 고정 표의 칸은 잘림 보기(`clipEnabled` 끔)에서도 넘친 글을 자른다.
//!
//! 한/글 12.30 라벨 문서의 크기 고정 칸은 화면·인쇄 모두 넘친 글을 감춘다(Mac 한글에는 잘림 보기가
//! 없다). 종전 rhwp는 칸 자름을 보기용 `ClipRect`로만 두어, 잘림 보기를 켠 캔버스가 넘친 글을 칸
//! 아래로 그렸다. 문서가 정한 자름은 보기 설정과 상관없이 지키는 `ClipPath`로 둔다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::wasm_api::HwpDocument;

/// 칸 높이를 정한 1×1 표에 넘치는 글을 넣는다.
fn overflowing_table(size_protect: bool) -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    let table: serde_json::Value =
        serde_json::from_str(&doc.create_table_native(0, 0, 0, 1, 1).unwrap()).unwrap();
    let para = table["paraIdx"].as_u64().unwrap() as u32;
    let control = table["controlIdx"].as_u64().unwrap() as u32;
    doc.set_cell_properties(0, para, control, 0, r#"{"height":3000}"#)
        .unwrap();
    if size_protect {
        doc.set_table_properties(0, para, control, r#"{"sizeProtect":true}"#)
            .unwrap();
    }
    doc.insert_text_in_cell_native(
        0,
        para as usize,
        control as usize,
        0,
        0,
        0,
        &"가나다라 ".repeat(80),
    )
    .unwrap();
    doc
}

fn clip_paths(doc: &mut HwpDocument) -> usize {
    // 앱은 잘림 보기를 켠 채(clipEnabled 끔) 시작한다.
    doc.set_clip_enabled(false);
    let tree = doc.get_page_layer_tree_native(0).unwrap();
    tree.matches("\"kind\":\"clipPath\"").count()
}

#[test]
fn size_protected_cell_clips_even_when_view_clipping_is_off() {
    assert_eq!(
        clip_paths(&mut overflowing_table(true)),
        1,
        "크기 고정 칸은 보기 설정과 상관없이 자른다"
    );
    assert_eq!(
        clip_paths(&mut overflowing_table(false)),
        0,
        "크기 고정이 아닌 표는 그대로 둔다"
    );
}
