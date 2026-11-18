//! 너비가 다른 단에 걸친 문단의 줄·캐럿.
//!
//! 한컴 12.30은 간격 8mm·단 너비 47.3mm/94.7mm의 두 단에 `다단 너비를 시험하는 글입니다. `를
//! 120번 이어 친 문단을 첫 단에서는 47.3mm로, 둘째 단으로 넘어간 634번째 글자부터는 94.7mm로
//! 접는다(저장 `horzsize` 13408 → 26844). 편집 경로가 문단 전체를 마지막 단 너비로만 접으면
//! 둘째 단의 글이 빠지거나 겹쳐 그려지고, 그 글자들의 캐럿이 본문 왼쪽 위로 떨어진다.

#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::control::Control;
use rhwp::wasm_api::HwpDocument;
use serde_json::Value;

const UNIT: &str = "다단 너비를 시험하는 글입니다. ";
/// 한컴 12.30 저장본에서 둘째 단 첫 줄이 시작하는 글자(0부터).
const SECOND_COLUMN_START: usize = 633;

fn json(text: String) -> Value {
    serde_json::from_str(&text).expect("JSON")
}

/// 단별 너비 47.3mm·94.7mm, 간격 8mm의 두 단 빈 문서에 시험 글을 120번 이어 친다.
/// `para`가 1이면 빈 첫 문단 뒤의 둘째 문단(단 정의 조판 부호가 없는 보통 문단)에 친다.
fn typed_document(para_idx: usize) -> (HwpDocument, usize) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.set_column_def_native(0, 2, 0, false, 2268).unwrap();
    for para in &mut doc.document_mut().sections[0].paragraphs {
        for control in &mut para.controls {
            if let Control::ColumnDef(column_def) = control {
                column_def.widths = vec![13408, 26844];
                column_def.gaps = vec![2268, 0];
                column_def.proportional_widths = false;
            }
        }
    }
    if para_idx == 1 {
        doc.split_paragraph_native(0, 0, 0, None).unwrap();
    }
    let mut len = 0;
    for _ in 0..120 {
        doc.insert_text_native(0, para_idx, len, UNIT).unwrap();
        len += UNIT.chars().count();
    }
    (doc, len)
}

struct Run {
    page: u32,
    start: usize,
    len: usize,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

fn runs(doc: &HwpDocument, para_idx: usize) -> Vec<Run> {
    let mut out = Vec::new();
    for page in 0..doc.page_count() {
        let layout = json(doc.get_page_text_layout_native(page).unwrap());
        for run in layout["runs"].as_array().unwrap() {
            let Some(start) = run["charStart"].as_u64() else {
                continue;
            };
            if run["paraIdx"].as_u64() != Some(para_idx as u64) {
                continue;
            }
            out.push(Run {
                page,
                start: start as usize,
                len: run["text"].as_str().unwrap().chars().count(),
                x: run["x"].as_f64().unwrap(),
                y: run["y"].as_f64().unwrap(),
                w: run["w"].as_f64().unwrap(),
                h: run["h"].as_f64().unwrap(),
            });
        }
    }
    out
}

fn caret(doc: &HwpDocument, para_idx: usize, offset: usize) -> (u32, f64, f64, f64) {
    let rect = json(doc.get_cursor_rect_native(0, para_idx, offset).unwrap());
    (
        rect["pageIndex"].as_u64().unwrap() as u32,
        rect["x"].as_f64().unwrap(),
        rect["y"].as_f64().unwrap(),
        rect["height"].as_f64().unwrap(),
    )
}

fn second_column_x(doc: &HwpDocument) -> f64 {
    json(doc.get_page_info_native(0).unwrap())["columns"][1]["x"]
        .as_f64()
        .unwrap()
}

/// 둘째 단 첫 줄이 시작하는 글자. 단 경계 앞뒤 줄의 너비가 달라야 한다.
fn second_column_start(doc: &HwpDocument, para_idx: usize) -> usize {
    let para = &doc.document().sections[0].paragraphs[para_idx];
    let boundary = (1..para.line_segs.len())
        .find(|&index| {
            para.line_segs[index].segment_width > para.line_segs[index - 1].segment_width
        })
        .unwrap_or_else(|| panic!("둘째 단 줄은 더 넓게 접는다: {}", dump(doc, para_idx)));
    para.char_offsets
        .partition_point(|&offset| offset < para.line_segs[boundary].text_start)
}

/// 실패 진단: 줄(시작 글자·너비·tag)과 단마다 그린 글자 범위.
fn dump(doc: &HwpDocument, para_idx: usize) -> String {
    let para = &doc.document().sections[0].paragraphs[para_idx];
    let lines: Vec<String> = para
        .line_segs
        .iter()
        .map(|line| {
            format!(
                "{}:{}:{:x}",
                para.char_offsets
                    .partition_point(|&offset| offset < line.text_start),
                line.segment_width,
                line.tag
            )
        })
        .collect();
    let drawn: Vec<String> = runs(doc, para_idx)
        .iter()
        .map(|run| format!("p{}x{:.0}:{}+{}", run.page, run.x, run.start, run.len))
        .collect();
    format!("lines {lines:?}\ndrawn {drawn:?}")
}

/// 단마다 그 단 너비로 접고, 화면이 문단의 모든 글자를 한 번씩 순서대로 그린다.
fn assert_folded_per_column(doc: &HwpDocument, para_idx: usize, len: usize, expected: usize) {
    let para = &doc.document().sections[0].paragraphs[para_idx];
    let start = second_column_start(doc, para_idx);
    assert_eq!(start, expected, "{}", dump(doc, para_idx));
    let boundary = para
        .line_segs
        .iter()
        .position(|line| para.char_offsets.partition_point(|&o| o < line.text_start) == start)
        .unwrap();
    let narrow = para.line_segs[boundary - 1].segment_width;
    let wide = para.line_segs[boundary].segment_width;
    assert!(
        para.line_segs[..boundary]
            .iter()
            .all(|line| line.segment_width == narrow),
        "첫 단의 줄은 모두 첫 단 너비로 접는다"
    );
    assert!(
        wide > narrow * 19 / 10,
        "둘째 단 줄은 둘째 단 너비로 접는다: {narrow} → {wide}"
    );
    let mut drawn = runs(doc, para_idx);
    drawn.sort_by_key(|run| run.start);
    let mut next = 0;
    for run in &drawn {
        assert_eq!(
            run.start,
            next,
            "빠지거나 겹쳐 그린 글자가 없어야 한다\n{}",
            dump(doc, para_idx)
        );
        next += run.len;
    }
    assert_eq!(next, len);
}

/// 모든 캐럿은 그 글자를 그린 줄 위에 놓인다.
fn assert_carets_on_drawn_lines(doc: &HwpDocument, para_idx: usize, len: usize) {
    let drawn = runs(doc, para_idx);
    for offset in 0..=len {
        let (page, x, y, _) = caret(doc, para_idx, offset);
        assert!(
            drawn.iter().any(|run| run.page == page
                && run.start <= offset
                && offset <= run.start + run.len
                && x >= run.x - 1.0
                && x <= run.x + run.w + 1.0
                && (y - run.y).abs() < run.h),
            "{offset}번째 캐럿 ({page}, {x}, {y})이 그린 줄 밖에 있다"
        );
    }
}

#[test]
fn unequal_columns_fold_each_column_at_its_own_width() {
    let (doc, len) = typed_document(0);
    assert_folded_per_column(&doc, 0, len, SECOND_COLUMN_START);
}

/// 단 정의가 없는 보통 문단도 같다. 빈 첫 문단이 한 줄을 차지해 한컴처럼 618번째 글자
/// (0부터 617)부터 둘째 단이다.
#[test]
fn body_paragraph_after_another_folds_and_places_carets_per_column() {
    let (doc, len) = typed_document(1);
    assert_folded_per_column(&doc, 1, len, SECOND_COLUMN_START - 16);
    assert_carets_on_drawn_lines(&doc, 1, len);
}

#[test]
fn caret_hit_and_navigation_follow_unequal_column_lines() {
    let (doc, len) = typed_document(0);
    let column_x = second_column_x(&doc);
    assert_carets_on_drawn_lines(&doc, 0, len);
    let (page, x, _, _) = caret(&doc, 0, SECOND_COLUMN_START + 1);
    assert_eq!(page, 0);
    assert!(x > column_x, "634번째 글자 뒤 캐럿은 둘째 단에 있다");

    // 둘째 단을 누르면 누른 자리의 글자로 간다.
    for offset in [SECOND_COLUMN_START + 1, 700, 1000, 1200] {
        let (page, x, y, height) = caret(&doc, 0, offset);
        let hit = json(
            doc.hit_test_native(page, x + 0.1, y + height / 2.0)
                .unwrap(),
        );
        assert_eq!(hit["charOffset"].as_u64().unwrap() as usize, offset);
    }

    // 줄 정보(Home/End)는 둘째 단 첫 줄을 634번째 글자부터 센다.
    let line = json(
        doc.get_line_info(0, 0, (SECOND_COLUMN_START + 5) as u32)
            .unwrap(),
    );
    assert_eq!(
        line["charStart"].as_u64().unwrap() as usize,
        SECOND_COLUMN_START
    );

    // 위아래 이동은 첫 단 끝 줄과 둘째 단 첫 줄 사이를 오간다.
    let (_, last_x, last_y, _) = caret(&doc, 0, SECOND_COLUMN_START - 3);
    let down = json(
        doc.move_vertical(
            0,
            0,
            (SECOND_COLUMN_START - 3) as u32,
            1,
            last_x,
            u32::MAX,
            0,
            0,
            0,
        )
        .unwrap(),
    );
    assert!(down["x"].as_f64().unwrap() >= column_x);
    assert!(down["y"].as_f64().unwrap() < last_y);
    let (_, first_x, first_y, _) = caret(&doc, 0, SECOND_COLUMN_START + 1);
    let up = json(
        doc.move_vertical(
            0,
            0,
            (SECOND_COLUMN_START + 1) as u32,
            -1,
            first_x,
            u32::MAX,
            0,
            0,
            0,
        )
        .unwrap(),
    );
    assert!(up["x"].as_f64().unwrap() < column_x);
    assert!(up["y"].as_f64().unwrap() > first_y);

    // 단 경계를 넘는 선택은 두 단에 모두 칠한다.
    let rects = json(doc.get_selection_rects(0, 0, 600, 0, 700).unwrap());
    let rects = rects.as_array().unwrap();
    assert!(rects.iter().any(|r| r["x"].as_f64().unwrap() < column_x));
    assert!(rects.iter().any(|r| r["x"].as_f64().unwrap() >= column_x));
    let covered: f64 = rects
        .iter()
        .filter(|r| r["x"].as_f64().unwrap() >= column_x)
        .map(|r| r["width"].as_f64().unwrap())
        .sum();
    assert!(covered > 300.0, "둘째 단의 선택 폭 {covered}");
}

#[test]
fn paragraph_pushed_down_by_enter_refolds_at_the_new_column_boundary() {
    let (mut doc, len) = typed_document(0);
    // 한컴 12.30에서 문단 맨 앞에 Enter를 치면 첫 단에 한 줄이 덜 들어, 둘째 단이 그 전
    // 첫 단 끝 줄(618번째 글자, 0부터 617)부터 넓게 접힌다.
    doc.split_paragraph_native(0, 0, 0, None).unwrap();
    assert_folded_per_column(&doc, 1, len, SECOND_COLUMN_START - 16);
    assert_carets_on_drawn_lines(&doc, 1, len);
}
