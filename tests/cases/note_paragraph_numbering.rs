//! 관계형. 각주·미주 문단의 문단 번호·글머리표·개요 머리를 본문처럼 그리고, 본문과 한 계수기로 센다.
//!
//! 한컴 한글 12.30(Mac) 인쇄 PDF(`pdf/note_paragraph_numbering/`, 2026-11-25 맥미니):
//! - 본문 `1. 본문 하나`·`2. 본문 둘1)`·`5. 본문 셋`, 각주 `3. 1) 주석 가`·`4. 주석 나`·`◆ 주석 다`.
//!   번호 머리는 각주 영역 왼쪽(본문 번호와 같은 x)에, 각주 번호 `1)`은 머리 영역 뒤
//!   (본문 번호 문단의 글 시작과 같은 x)에 놓인다.
//! - 미주도 문서 끝에 그리지만 번호는 참조 자리에서 센다: 미주 `3.`·`4.`, 이어지는 본문 `5.`.
//! - 표 칸 안 각주도 칸 문단 뒤 그 자리에서 센다: `2. 칸 가1)` → 각주 `3.` → `4. 칸 나` → `5. 본문 둘`.
//!
//! `footnote-hancom.hwpx`는 한컴 12.30에서 둘째·셋째 각주 문단에 서식 > 문단 번호 적용/해제와
//! 글머리표 적용/해제를 눌러 저장한 문서다(한컴은 본문 번호 정의 1을 그대로 쓰고 본문 셋째 문단을 4로 바꾼다).
#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;

use rhwp::document_core::DocumentCore;
use rhwp::renderer::render_tree::{BoundingBox, RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

/// 각주 영역 안과 밖의 글자를 공백 없이 모으고, 글자 조각의 왼쪽 x를 함께 남긴다.
#[derive(Default)]
struct Texts {
    note: String,
    body: String,
    note_runs: Vec<(String, BoundingBox)>,
    body_runs: Vec<(String, BoundingBox)>,
}

fn collect(node: &RenderNode, in_note: bool, out: &mut Texts) {
    let in_note = in_note || matches!(node.node_type, RenderNodeType::FootnoteArea);
    if let RenderNodeType::TextRun(run) = &node.node_type {
        let text: String = run
            .display_or_text()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let (all, runs) = if in_note {
            (&mut out.note, &mut out.note_runs)
        } else {
            (&mut out.body, &mut out.body_runs)
        };
        all.push_str(&text);
        if !text.is_empty() {
            runs.push((text, node.bbox));
        }
    }
    for child in &node.children {
        collect(child, in_note, out);
    }
}

fn texts(core: &DocumentCore) -> Texts {
    let tree = core.build_page_render_tree(0).expect("render tree");
    let mut out = Texts::default();
    collect(&tree.root, false, &mut out);
    out
}

fn open(name: &str) -> DocumentCore {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/note_paragraph_numbering")
        .join(name);
    DocumentCore::from_bytes(&std::fs::read(path).expect("read fixture")).expect("open")
}

fn x_of<'a>(runs: &'a [(String, BoundingBox)], starts: &str) -> f64 {
    runs.iter()
        .find(|(text, _)| text.starts_with(starts))
        .unwrap_or_else(|| panic!("{starts:?} 조각 없음: {runs:?}"))
        .1
        .x
}

/// 본문 세 번호 문단의 둘째에 각주를 달고, 각주 문단 셋을 번호·번호·글머리표로 바꾼다.
fn edited_footnote() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, "본문 하나").unwrap();
    doc.split_paragraph_native(0, 0, 5, None).unwrap();
    doc.insert_text_native(0, 1, 0, "본문 둘").unwrap();
    doc.split_paragraph_native(0, 1, 4, None).unwrap();
    doc.insert_text_native(0, 2, 0, "본문 셋").unwrap();
    let number = doc.ensure_default_numbering();
    let bullet = doc.ensure_default_bullet("◆");
    let head = |kind: &str, id: u16| {
        format!(r#"{{"headType":"{kind}","numberingId":{id},"paraLevel":0}}"#)
    };
    for para in 0..3 {
        doc.apply_para_format_native(0, para, &head("Number", number))
            .unwrap();
    }
    let raw = doc.insert_footnote_native(0, 1, 4).unwrap();
    let control: usize = serde_json::from_str::<serde_json::Value>(&raw).unwrap()["controlIdx"]
        .as_u64()
        .unwrap() as usize;
    doc.insert_text_in_footnote_native(0, 1, control, 0, 2, "주석 가")
        .unwrap();
    doc.split_paragraph_in_footnote_native(0, 1, control, 0, 6, None)
        .unwrap();
    doc.insert_text_in_footnote_native(0, 1, control, 1, 0, "주석 나")
        .unwrap();
    doc.split_paragraph_in_footnote_native(0, 1, control, 1, 4, None)
        .unwrap();
    doc.insert_text_in_footnote_native(0, 1, control, 2, 0, "주석 다")
        .unwrap();
    for (para, kind, id) in [
        (0, "Number", number),
        (1, "Number", number),
        (2, "Bullet", bullet),
    ] {
        doc.apply_para_format_in_footnote_native(0, 1, control, para, &head(kind, id))
            .unwrap();
    }
    doc
}

#[test]
fn edited_footnote_paragraphs_draw_number_and_bullet_heads() {
    let doc = edited_footnote();
    let out = texts(&doc);
    assert!(
        out.note.contains("3.1)주석가4.주석나◆주석다"),
        "각주 머리: {}",
        out.note
    );
    for expected in ["1.본문하나", "2.본문둘", "5.본문셋"] {
        assert!(
            out.body.contains(expected),
            "본문 {expected:?}: {}",
            out.body
        );
    }
    // 머리는 본문 번호와 같은 x, 각주 번호는 본문 번호 문단의 글과 같은 x에 놓인다.
    let body_head = x_of(&out.body_runs, "1.");
    let body_text = x_of(&out.body_runs, "본문");
    assert!((x_of(&out.note_runs, "3.") - body_head).abs() < 0.5);
    assert!((x_of(&out.note_runs, "1)") - body_text).abs() < 0.5);
    assert!((x_of(&out.note_runs, "4.") - body_head).abs() < 0.5);
    assert!(x_of(&out.note_runs, "◆") < x_of(&out.note_runs, "주석다") - 10.0);

    // HWP·HWPX로 저장해 다시 열어도 같은 머리와 번호를 그린다.
    for (format, bytes) in [
        ("HWP", doc.export_hwp().unwrap()),
        ("HWPX", doc.export_hwpx().unwrap()),
    ] {
        let reopened = DocumentCore::from_bytes(&bytes).unwrap();
        let again = texts(&reopened);
        assert!(
            again.note.contains("3.1)주석가4.주석나◆주석다"),
            "{format} 각주 머리: {}",
            again.note
        );
        assert!(
            again.body.contains("5.본문셋"),
            "{format} 본문: {}",
            again.body
        );
    }
}

#[test]
fn hancom_saved_footnote_heads_share_body_counter() {
    let out = texts(&open("footnote-hancom.hwpx"));
    assert!(
        out.note.contains("1)주석가3.주석나◆주석다"),
        "각주 머리: {}",
        out.note
    );
    for expected in ["1.본문하나", "2.본문둘", "4.본문셋"] {
        assert!(
            out.body.contains(expected),
            "본문 {expected:?}: {}",
            out.body
        );
    }
}

#[test]
fn endnote_heads_count_at_reference_position() {
    let out = texts(&open("endnote.hwpx"));
    for expected in ["2.본문둘", "5.본문셋", "3.1)주석가4.주석나◆주석다"] {
        assert!(out.body.contains(expected), "{expected:?}: {}", out.body);
    }
}

#[test]
fn cell_footnote_heads_count_after_their_cell_paragraph() {
    let out = texts(&open("cell-footnote.hwpx"));
    assert!(
        out.note.contains("3.1)셀주석가◆셀주석나"),
        "각주 머리: {}",
        out.note
    );
    for expected in ["1.본문하나", "2.칸가", "4.칸나", "5.본문둘"] {
        assert!(
            out.body.contains(expected),
            "본문 {expected:?}: {}",
            out.body
        );
    }
}
