//! 쪽 > 새 번호로 시작을 맞붙은 개체 앞 캐럿 자리에 넣는다.
//!
//! 글자처럼 취급한 표 세 개가 한 글자 자리에 맞붙은 문단(가운데 표만 캡션 없음, 한컴2020 인쇄 "표 1"·"표 3")에서
//! 셋째 표 앞 캐럿에 표 번호 2를 넣으면 그 표 캡션이 2가 되고, HWP·HWPX 저장 뒤에도 남는다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::{
    document_core::DocumentCore,
    model::control::{AutoNumberType, Control},
};

fn open(bytes: &[u8]) -> DocumentCore {
    DocumentCore::from_bytes(bytes).unwrap()
}

fn open_fixture() -> DocumentCore {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/issue1470_authored_caption_numbering/tables-caption-removed.hwpx");
    open(&std::fs::read(path).unwrap())
}

fn painted(core: &DocumentCore) -> Vec<u16> {
    use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
    fn walk(n: &RenderNode, caption: bool, out: &mut Vec<u16>) {
        let caption = if let RenderNodeType::TextLine(l) = &n.node_type {
            l.caption_owner.is_some()
        } else {
            caption
        };
        if caption {
            if let RenderNodeType::TextRun(r) = &n.node_type {
                if let Some(value) = r.display_text.as_deref().and_then(|s| s.parse().ok()) {
                    out.push(value)
                }
            }
        }
        for child in &n.children {
            walk(child, caption, out)
        }
    }
    let mut out = Vec::new();
    for page in 0..core.page_count() {
        walk(
            &core.build_page_render_tree(page).unwrap().root,
            false,
            &mut out,
        )
    }
    out
}

/// 셋째 표의 글자 자리와 그 자리에서 표 앞에 맞붙은 논리 개체 수.
fn third_table_slot(core: &DocumentCore) -> (usize, usize) {
    let para = &core.document().sections[0].paragraphs[0];
    let positions = para.control_text_positions();
    let index = para
        .controls
        .iter()
        .enumerate()
        .filter(|(_, c)| matches!(c, Control::Table(_)))
        .nth(2)
        .unwrap()
        .0;
    let before = (0..index)
        .filter(|&i| positions[i] == positions[index] && para.controls[i].is_logical_inline())
        .count();
    (positions[index], before)
}

#[test]
fn new_number_before_adjacent_table_renumbers_that_table() {
    let mut core = open_fixture();
    assert_eq!(painted(&core), [1, 3]);
    let (offset, before) = third_table_slot(&core);

    core.insert_new_number_in_slot_native(0, 0, offset, Some(before), 2, AutoNumberType::Table)
        .unwrap();
    assert_eq!(painted(&core), [1, 2]);
    for bytes in [
        core.export_hwp_native().unwrap(),
        core.export_hwpx_native().unwrap(),
    ] {
        assert_eq!(painted(&open(&bytes)), [1, 2]);
    }
}

#[test]
fn new_number_without_slot_stays_after_adjacent_objects() {
    let mut core = open_fixture();
    let (offset, _) = third_table_slot(&core);
    // 예전 호출(자리 없음)은 그 글자 자리 개체 모두 뒤에 넣어 표 번호가 그대로다.
    core.insert_new_number_of_kind_native(0, 0, offset, 2, AutoNumberType::Table)
        .unwrap();
    assert_eq!(painted(&core), [1, 3]);
}
