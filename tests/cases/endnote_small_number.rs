//! 관계형. 미주 모양의 내용 번호 속성 "작게"는 미주 첫 문단 앞 번호를 작은 위첨자로 그리고 뒤 글을 그만큼
//! 당긴다. 한/글 12.30(Mac) 화면은 작게면 미주 안의 "1)"을 본문 참조 번호처럼 작게 올려 그린다
//! (2026-11-11 화면 확인). 보통이면 번호는 본문 글자와 같은 크기다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};

/// (글, 글자 크기, x, 너비, 기준선) — 미주 문단(가상 문단 번호가 본문 문단 수 이상)의 글 조각.
fn endnote_runs(core: &DocumentCore) -> Vec<(String, f64, f64, f64, f64)> {
    fn walk(node: &RenderNode, body_paras: usize, out: &mut Vec<(String, f64, f64, f64, f64)>) {
        if let RenderNodeType::TextRun(run) = &node.node_type {
            if run.para_index.is_some_and(|p| p >= body_paras) && !run.text.trim().is_empty() {
                out.push((
                    run.text.clone(),
                    run.style.font_size,
                    node.bbox.x,
                    node.bbox.width,
                    run.baseline,
                ));
            }
        }
        for child in &node.children {
            walk(child, body_paras, out);
        }
    }
    let tree = core.build_page_render_tree(0).unwrap();
    let mut out = Vec::new();
    walk(
        &tree.root,
        core.document().sections[0].paragraphs.len(),
        &mut out,
    );
    out
}

fn sample(small: bool) -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.insert_text_native(0, 0, 0, "본문 글").unwrap();
    core.insert_endnote_native(0, 0, 2).unwrap();
    let control = core.document().sections[0].paragraphs[0]
        .controls
        .iter()
        .position(|c| matches!(c, Control::Endnote(_)))
        .unwrap();
    let len = core.document().sections[0].paragraphs[0].controls[control].clone();
    let Control::Endnote(note) = len else {
        unreachable!()
    };
    let end = note.paragraphs[0].text.chars().count();
    core.insert_text_in_footnote_native(0, 0, control, 0, end, "미주 내용")
        .unwrap();
    core.apply_endnote_shape_native(0, &format!(r#"{{"numberCodeSuperscript":{small}}}"#))
        .unwrap();
    core
}

#[test]
fn small_endnote_number_is_drawn_as_a_raised_smaller_run() {
    let core = sample(true);
    let runs = endnote_runs(&core);
    let marker = runs
        .iter()
        .position(|(text, ..)| text.trim() == "1)")
        .unwrap_or_else(|| panic!("번호 조각: {runs:?}"));
    let body = runs
        .iter()
        .skip(marker + 1)
        .find(|(text, ..)| text.contains("미주"))
        .unwrap_or_else(|| panic!("본문 조각: {runs:?}"));
    let (_, size, x, width, baseline) = &runs[marker];
    assert!(*size < body.1 * 0.9, "번호가 작다: {runs:?}");
    assert!(*baseline < body.4, "번호가 올라간다: {runs:?}");
    assert!(
        body.2 <= x + width + body.1,
        "뒤 글이 줄인 번호에 붙는다: {runs:?}"
    );

    // 보통이면 번호를 줄이지 않는다.
    let normal = endnote_runs(&sample(false));
    let first = normal
        .iter()
        .find(|(text, ..)| text.contains("1)"))
        .unwrap_or_else(|| panic!("번호: {normal:?}"));
    let body = normal
        .iter()
        .find(|(text, ..)| text.contains("미주"))
        .unwrap();
    assert!((first.1 - body.1).abs() < 0.01, "{normal:?}");
}
