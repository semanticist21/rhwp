//! 문단 모양의 한 줄로 입력·세로 정렬·최소 공백은 적용 즉시 줄을 다시 짜고,
//! 외톨이줄 보호·문단 보호는 본문 쪽 나눔에 쓰인다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use serde_json::Value;

const LONG: &str = "가나다 라마바 사아자 차카타 파하 ";

fn one_paragraph(before: Option<&str>) -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    if let Some(props) = before {
        core.apply_para_format_native(0, 0, props).unwrap();
    }
    core.insert_text_native(0, 0, 0, &LONG.repeat(12)).unwrap();
    core
}

fn line_starts(core: &DocumentCore, para: usize) -> Vec<u32> {
    core.document().sections[0].paragraphs[para]
        .line_segs
        .iter()
        .map(|line| line.text_start)
        .collect()
}

/// 글을 넣은 뒤 적용한 값은 먼저 적용하고 글을 넣은 결과와 같은 줄을 만들어야 한다.
fn assert_apply_matches_typed(props: &str) -> DocumentCore {
    let typed = one_paragraph(Some(props));
    let mut applied = one_paragraph(None);
    applied.apply_para_format_native(0, 0, props).unwrap();
    assert_eq!(
        line_starts(&applied, 0),
        line_starts(&typed, 0),
        "{props}: 적용 뒤 줄 나눔"
    );
    assert_eq!(
        applied.document().sections[0].paragraphs[0]
            .line_segs
            .iter()
            .map(|line| line.baseline_distance)
            .collect::<Vec<_>>(),
        typed.document().sections[0].paragraphs[0]
            .line_segs
            .iter()
            .map(|line| line.baseline_distance)
            .collect::<Vec<_>>(),
        "{props}: 적용 뒤 기준선"
    );
    applied
}

fn props(core: &DocumentCore) -> Value {
    serde_json::from_str(&core.get_para_properties_at_native(0, 0).unwrap()).unwrap()
}

#[test]
fn single_line_reflows_on_apply() {
    assert!(line_starts(&one_paragraph(None), 0).len() > 2);
    let core = assert_apply_matches_typed(r#"{"singleLine":true}"#);
    assert_eq!(line_starts(&core, 0).len(), 1);
    assert_eq!(props(&core)["singleLine"], true);
}

#[test]
fn vertical_align_republishes_baselines_on_apply() {
    let core = assert_apply_matches_typed(r#"{"verticalAlign":2}"#);
    let line = &core.document().sections[0].paragraphs[0].line_segs[0];
    assert_eq!(line.baseline_distance, line.text_height / 2);
    assert_eq!(props(&core)["verticalAlign"], 2);
}

#[test]
fn condense_is_applied_reflowed_and_roundtrips() {
    let plain = line_starts(&one_paragraph(None), 0);
    let core = assert_apply_matches_typed(r#"{"condense":75}"#);
    assert_ne!(
        line_starts(&core, 0),
        plain,
        "최소 공백이 줄 나눔을 바꾸는 글"
    );
    assert_eq!(props(&core)["condense"], 75);
    // 범위 밖 값은 한컴 상한(75)으로 막는다.
    let mut clamped = one_paragraph(None);
    clamped
        .apply_para_format_native(0, 0, r#"{"condense":120}"#)
        .unwrap();
    assert_eq!(props(&clamped)["condense"], 75);
    for bytes in [
        core.export_hwp_native().unwrap(),
        core.export_hwpx_native().unwrap(),
    ] {
        let reopened = DocumentCore::from_bytes(&bytes).unwrap();
        assert_eq!(props(&reopened)["condense"], 75);
        assert_eq!(line_starts(&reopened, 0), line_starts(&core, 0));
    }
}

/// 짧은 문단 80개 사이 `long` 번째에 여러 줄 문단을 둔다.
fn paged(long: usize, props: Option<&str>) -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.begin_batch_native().unwrap();
    for index in 0..80 {
        if index > 0 {
            core.insert_paragraph_native(0, index).unwrap();
        }
        let text = if index == long {
            LONG.repeat(10)
        } else {
            format!("짧은 문단 {index:02}")
        };
        core.insert_text_native(0, index, 0, &text).unwrap();
    }
    if let Some(props) = props {
        core.apply_para_format_native(0, long, props).unwrap();
    }
    core.end_batch_native().unwrap();
    core
}

/// 여러 줄 문단의 줄마다 놓인 쪽. 줄 시작 자리의 캐럿은 앞 줄 끝에 붙으므로 한 글자 안쪽을 잰다.
fn line_pages(core: &DocumentCore, para: usize) -> Vec<u64> {
    line_starts(core, para)
        .into_iter()
        .map(|start| {
            let rect: Value = serde_json::from_str(
                &core
                    .get_cursor_rect_native(0, para, start as usize + 1)
                    .unwrap(),
            )
            .unwrap();
            rect["pageIndex"].as_u64().unwrap()
        })
        .collect()
}

fn first_page_lines(pages: &[u64]) -> usize {
    pages.iter().filter(|&&page| page == pages[0]).count()
}

/// 첫 쪽에 `head`줄이 남는 자리를 찾는다.
fn split_with_head(head: impl Fn(usize, usize) -> bool) -> usize {
    (20..80)
        .find(|&long| {
            let pages = line_pages(&paged(long, None), long);
            let first = first_page_lines(&pages);
            first < pages.len() && head(first, pages.len())
        })
        .expect("쪽 경계에 걸린 문단")
}

#[test]
fn widow_orphan_moves_single_first_line() {
    let long = split_with_head(|first, _| first == 1);
    let pages = line_pages(&paged(long, Some(r#"{"widowOrphan":true}"#)), long);
    assert_eq!(
        first_page_lines(&pages),
        pages.len(),
        "외톨이 첫 줄은 다음 쪽으로"
    );
}

#[test]
fn widow_orphan_keeps_two_lines_on_next_page() {
    let long = split_with_head(|first, total| total - first == 1 && first > 2);
    let plain = line_pages(&paged(long, None), long);
    let core = paged(long, Some(r#"{"widowOrphan":true}"#));
    let pages = line_pages(&core, long);
    assert_eq!(first_page_lines(&pages), first_page_lines(&plain) - 1);
    for bytes in [
        core.export_hwp_native().unwrap(),
        core.export_hwpx_native().unwrap(),
    ] {
        let reopened = DocumentCore::from_bytes(&bytes).unwrap();
        assert_eq!(line_pages(&reopened, long), pages);
    }
}

#[test]
fn keep_lines_moves_split_paragraph_whole() {
    let long = split_with_head(|first, total| first >= 2 && total - first >= 2);
    let pages = line_pages(&paged(long, Some(r#"{"keepLines":true}"#)), long);
    assert_eq!(
        first_page_lines(&pages),
        pages.len(),
        "문단 보호는 통째로 넘긴다"
    );
}
