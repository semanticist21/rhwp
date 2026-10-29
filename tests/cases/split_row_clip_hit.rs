//! 쪽에서 나뉜 행의 칸은 위아래가 잘려 그려진다. 잘려 보이지 않는 줄은 클릭으로
//! 고를 수 없고, 그 자리를 누르면 같은 칸의 보이는 줄로 간다.
//!
//! 원래 rowbreak 문서는 공식 997e2599e 이후 가려진 줄이 없다. 이 입력은 d776532a의
//! createBlankDocument → createTable(1×1) → "클립 검사 ".repeat(180) 입력 →
//! fontSize=3600, charOffsets=[127;7] 적용 → exportHwpx로 만들었다. 127은 API가
//! 받아 보존하는 i8 값이지만 명목 범위(-100..100) 밖인 가져오기 스트레스 입력이다.
//! 마지막 조각의 본문 꼬리 대신 중간 쪽의 실제 분할 칸을 검사한다.

use rhwp::model::control::Control;
use rhwp::model::table::TablePageBreak;
use rhwp::renderer::render_tree::{BoundingBox, RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;
use serde_json::Value;

const SAMPLE: &str = "tests/fixtures/split_row_clip_hit/shifted-cell-lines.hwpx";
const PAGE: u32 = 1;

/// 쪽 조각 칸의 clip(위, 아래) 안팎으로 나눈 글자 런 bbox.
fn clipped_runs(
    node: &RenderNode,
    clip: Option<(f64, f64)>,
    out: &mut Vec<(BoundingBox, f64, f64)>,
) {
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
        let b = &node.bbox;
        if !run.text.trim().is_empty() && (b.y > bottom || b.y + b.height < top) {
            out.push((b.clone(), top, bottom));
        }
    }
    for child in &node.children {
        clipped_runs(child, clip, out);
    }
}

fn split_cell(node: &RenderNode) -> Option<&RenderNode> {
    if matches!(&node.node_type, RenderNodeType::TableCell(cell) if cell.clip && cell.page_fragment)
    {
        return Some(node);
    }
    node.children.iter().find_map(split_cell)
}

#[test]
fn clicking_a_line_clipped_off_a_split_row_lands_on_a_visible_line() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(SAMPLE);
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
        PAGE > 0 && PAGE + 1 < doc.page_count(),
        "중간 분할 쪽을 검사한다"
    );
    let tree = doc.build_page_render_tree(PAGE).unwrap();
    let cell = split_cell(&tree.root).expect("clip이 있는 실제 쪽 분할 칸");
    let mut hidden = Vec::new();
    clipped_runs(&tree.root, None, &mut hidden);
    assert!(
        !hidden.is_empty(),
        "재현 문서가 더는 칸 clip 밖에 줄을 두지 않는다"
    );
    for (bbox, top, bottom) in hidden {
        assert!(bbox.x < cell.bbox.x + cell.bbox.width && bbox.x + bbox.width > cell.bbox.x);
        let mid_x = bbox.x + bbox.width / 2.0;
        assert!((cell.bbox.x..=cell.bbox.x + cell.bbox.width).contains(&mid_x));
        let (x, y) = (bbox.x + bbox.width / 2.0, bbox.y + bbox.height / 2.0);
        let hit: Value = serde_json::from_str(&doc.hit_test_native(PAGE, x, y).unwrap()).unwrap();
        assert_eq!(hit["sectionIndex"], 0, "{hit}");
        assert_eq!(hit["parentParaIndex"], parent, "{hit}");
        assert_eq!(hit["controlIndex"], control, "{hit}");
        assert_eq!(hit["cellIndex"], 0, "{hit}");
        assert_eq!(hit["cellParaIndex"], 0, "{hit}");
        let rect = &hit["cursorRect"];
        assert_eq!(rect["pageIndex"], PAGE, "{hit}");
        let caret_mid = rect["y"].as_f64().unwrap() + rect["height"].as_f64().unwrap() / 2.0;
        assert!(
            (top..=bottom).contains(&caret_mid),
            "({x:.1}, {y:.1}) 클릭이 clip {top:.1}..{bottom:.1} 밖 캐럿으로 갔다: {hit}"
        );
    }
}
