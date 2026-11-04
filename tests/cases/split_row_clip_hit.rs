//! 관계형: split_row_clip_hit의 공개 입력·편집·저장 계약.
//! 주석 갱신: 2026-11-04T00:23:31+09:00; 직전 파일 변경 커밋: 2026-11-03T21:45:03+09:00 39b4f81f6aab4cc08c6145ed6911db2a8fe5125e.
//! 측정: 2026-11-03, 96 DPI·색상 경계 보정 전 2px raw 실루엣. rowbreak-problem-pages.hwpx 전 18쪽: Native/fresh WASM 각각 최저 92.88%; 16쪽 98.37%/98.49%. 독립 Print 확대 판독 완료. 절대 좌표 핀 없이 실제 clip과 셀 경로·마지막 내용 보존을 검사한다.
//! 정식 증거: mydocs/pr/assets/semanticist21-20261103/validation.md 및 전쪽 TSV·확대 PNG.
use rhwp::model::control::Control;
use rhwp::model::table::TablePageBreak;
use rhwp::renderer::layout::CellContext;
use rhwp::renderer::render_tree::{BoundingBox, RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;
use serde_json::{json, Value};

const SAMPLE: &str = "samples/rowbreak-problem-pages.hwpx";
const PAGE: u32 = 15;

struct ClippedRun {
    bbox: BoundingBox,
    top: f64,
    bottom: f64,
    context: CellContext,
    text: String,
}
fn clipped_runs(node: &RenderNode, clip: Option<(f64, f64)>, out: &mut Vec<ClippedRun>) {
    let clip = match &node.node_type {
        RenderNodeType::TableCell(cell) if cell.clip => {
            let (top, bottom) = (node.bbox.y, node.bbox.y + node.bbox.height);
            Some(clip.map_or((top, bottom), |(t, b): (f64, f64)| {
                (t.max(top), b.min(bottom))
            }))
        }
        _ => clip,
    };
    if let (RenderNodeType::TextRun(run), Some((top, bottom))) = (&node.node_type, clip) {
        if !run.text.trim().is_empty() {
            if let Some(context) = &run.cell_context {
                out.push(ClippedRun {
                    bbox: node.bbox,
                    top,
                    bottom,
                    context: context.clone(),
                    text: run.text.clone(),
                });
            }
        }
    }
    for child in &node.children {
        clipped_runs(child, clip, out);
    }
}
#[test]
fn split_row_visible_lines_keep_content_and_hit_ownership() {
    let doc = HwpDocument::from_bytes(&std::fs::read(SAMPLE).unwrap()).unwrap();
    let tree = doc.build_page_render_tree(PAGE).unwrap();
    let mut runs = Vec::new();
    clipped_runs(&tree.root, None, &mut runs);
    let visible: Vec<_> = runs
        .iter()
        .filter(|r| r.bbox.y < r.bottom && r.bbox.y + r.bbox.height > r.top)
        .collect();
    assert!(
        !visible.is_empty(),
        "분할 셀의 보이는 내용은 반드시 검사한다"
    );
    let text: String = visible.iter().map(|r| r.text.as_str()).collect();
    assert!(text.contains("공공데이터"), "마지막 문단 보존: {text}");
    assert!(text.contains("고려하여 추진"), "마지막 문단 보존: {text}");
    for run in &runs {
        let b = &run.bbox;
        if b.width <= 0.0 {
            continue;
        }
        let top = b.y.max(run.top);
        let bottom = (b.y + b.height).min(run.bottom);
        let visible = top < bottom;
        let x = b.x + b.width / 2.0;
        let y = if visible {
            (top + bottom) / 2.0
        } else {
            b.y + b.height / 2.0
        };
        let hit: Value = serde_json::from_str(&doc.hit_test_native(PAGE, x, y).unwrap()).unwrap();
        let rect = &hit["cursorRect"];
        assert_eq!(rect["pageIndex"], PAGE, "{hit}");
        let mid = rect["y"].as_f64().unwrap() + rect["height"].as_f64().unwrap() / 2.0;
        assert!((run.top..=run.bottom).contains(&mid), "clip 밖 캐럿: {hit}");
        if visible {
            let expected: Vec<_> = run.context.path.iter().map(|p| json!({"controlIndex":p.control_index,"cellIndex":p.cell_index,"cellParaIndex":p.cell_para_index})).collect();
            assert_eq!(
                hit["parentParaIndex"], run.context.parent_para_index,
                "{hit}"
            );
            assert_eq!(hit["cellPath"], json!(expected), "{hit}");
        }
    }
}

// 원래 rowbreak 문서는 공식 997e2599e 이후 칸 clip 밖에 가려진 줄이 없다. 이 입력은
// createBlankDocument → createTable(1×1) → "클립 검사 ".repeat(180) 입력 →
// fontSize=3600, charOffsets=[127;7] 적용 → exportHwpx로 만들었다. 127은 API가
// 받아 보존하는 i8 값이지만 명목 범위(-100..100) 밖인 가져오기 스트레스 입력이다.
// 마지막 조각의 본문 꼬리 대신 중간 쪽의 실제 분할 칸에서 가려진 줄 클릭을 검사한다.
const SHIFTED_SAMPLE: &str = "tests/fixtures/split_row_clip_hit/shifted-cell-lines.hwpx";
const SHIFTED_PAGE: u32 = 1;

fn split_cell(node: &RenderNode) -> Option<&RenderNode> {
    if matches!(&node.node_type, RenderNodeType::TableCell(cell) if cell.clip && cell.page_fragment)
    {
        return Some(node);
    }
    node.children.iter().find_map(split_cell)
}

#[test]
fn clicking_a_line_clipped_off_a_split_row_lands_on_a_visible_line() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(SHIFTED_SAMPLE);
    let doc = HwpDocument::from_bytes(&std::fs::read(path).unwrap()).unwrap();
    let document = doc.document();
    assert_eq!(document.sections.len(), 1);
    let tables: Vec<_> = document.sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .flat_map(|(pi, para)| {
            para.controls
                .iter()
                .enumerate()
                .filter_map(move |(ci, control)| {
                    if let Control::Table(table) = control {
                        Some((pi, ci, table))
                    } else {
                        None
                    }
                })
        })
        .collect();
    assert_eq!(tables.len(), 1, "숨긴 런과 클릭의 표 소유가 유일하다");
    let (parent, control, table) = tables[0];
    assert_eq!(
        (table.row_count, table.col_count, table.cells.len()),
        (1, 1, 1)
    );
    assert_eq!(table.page_break, TablePageBreak::RowBreak);
    let paragraphs = &table.cells[0].paragraphs;
    assert_eq!(paragraphs.len(), 1);
    assert_eq!(paragraphs[0].text, "클립 검사 ".repeat(180));
    assert_eq!(paragraphs[0].char_shapes[0].start_pos, 0);
    for run in &paragraphs[0].char_shapes {
        if run.start_pos < paragraphs[0].text.chars().count() as u32 {
            let shape = &document.doc_info.char_shapes[run.char_shape_id as usize];
            assert_eq!(shape.base_size, 3600);
            assert_eq!(shape.char_offsets, [127; 7]);
        }
    }
    assert!(
        SHIFTED_PAGE > 0 && SHIFTED_PAGE + 1 < doc.page_count(),
        "중간 분할 쪽을 검사한다"
    );
    let tree = doc.build_page_render_tree(SHIFTED_PAGE).unwrap();
    let cell = split_cell(&tree.root).expect("clip이 있는 실제 쪽 분할 칸");
    let mut runs = Vec::new();
    clipped_runs(&tree.root, None, &mut runs);
    let hidden: Vec<_> = runs
        .iter()
        .filter(|r| r.bbox.y > r.bottom || r.bbox.y + r.bbox.height < r.top)
        .map(|r| (r.bbox, r.top, r.bottom))
        .collect();
    assert!(
        !hidden.is_empty(),
        "재현 문서가 더는 칸 clip 밖에 줄을 두지 않는다"
    );
    for (bbox, top, bottom) in hidden {
        assert!(bbox.x < cell.bbox.x + cell.bbox.width && bbox.x + bbox.width > cell.bbox.x);
        let mid_x = bbox.x + bbox.width / 2.0;
        assert!((cell.bbox.x..=cell.bbox.x + cell.bbox.width).contains(&mid_x));
        let (x, y) = (bbox.x + bbox.width / 2.0, bbox.y + bbox.height / 2.0);
        let hit: Value =
            serde_json::from_str(&doc.hit_test_native(SHIFTED_PAGE, x, y).unwrap()).unwrap();
        assert_eq!(hit["sectionIndex"], 0, "{hit}");
        assert_eq!(hit["parentParaIndex"], parent, "{hit}");
        assert_eq!(hit["controlIndex"], control, "{hit}");
        assert_eq!(hit["cellIndex"], 0, "{hit}");
        assert_eq!(hit["cellParaIndex"], 0, "{hit}");
        let rect = &hit["cursorRect"];
        assert_eq!(rect["pageIndex"], SHIFTED_PAGE, "{hit}");
        let caret_mid = rect["y"].as_f64().unwrap() + rect["height"].as_f64().unwrap() / 2.0;
        assert!(
            (top..=bottom).contains(&caret_mid),
            "({x:.1}, {y:.1}) 클릭이 clip {top:.1}..{bottom:.1} 밖 캐럿으로 갔다: {hit}"
        );
    }
}
