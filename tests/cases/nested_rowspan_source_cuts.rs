//! 중첩 병합 셀의 원본 줄 컷과 캡션 예약을 공개 문서 조판 경로로 검사한다.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeMap;

use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::model::document::Section;
use rhwp::model::page::PageDef;
use rhwp::model::paragraph::{LineSeg, Paragraph};
use rhwp::model::shape::{Caption, CaptionDirection, TextWrap, VertRelTo};
use rhwp::model::style::ParaShape;
use rhwp::model::table::{Cell, Table, TablePageBreak, VerticalAlign};
use rhwp::renderer::render_tree::{BoundingBox, RenderNode, RenderNodeType};
use serde_json::Value;

fn paragraph(text: &str, lines: usize, reset: Option<bool>) -> Paragraph {
    assert_eq!(text.chars().count(), lines);
    Paragraph {
        text: text.to_owned(),
        char_count: lines as u32 + 1,
        char_offsets: (0..lines as u32).collect(),
        cell_vpos_reset: reset,
        line_segs: (0..lines)
            .map(|line| LineSeg {
                text_start: line as u32,
                vertical_pos: line as i32 * 1200,
                line_height: 1200,
                text_height: 1200,
                baseline_distance: 960,
                segment_width: 10_000,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

fn cell(row: u16, col: u16, paragraphs: Vec<Paragraph>) -> Cell {
    Cell {
        row,
        col,
        row_span: 1,
        col_span: 1,
        width: 10_000,
        height: 1200,
        vertical_align: VerticalAlign::Top,
        paragraphs,
        ..Default::default()
    }
}

fn fixture(reset: bool, caption: Option<CaptionDirection>, body_height: u32) -> DocumentCore {
    let mut merged = cell(
        0,
        0,
        vec![paragraph("A", 1, None), paragraph("B", 1, Some(reset))],
    );
    merged.row_span = 2;
    let mut child = Table {
        row_count: 2,
        col_count: 2,
        page_break: TablePageBreak::RowBreak,
        cells: vec![
            merged,
            cell(0, 1, vec![paragraph("R", 1, None)]),
            cell(1, 1, vec![paragraph("0123456789", 10, None)]),
        ],
        caption: caption.map(|direction| Caption {
            direction,
            width: 10_000,
            max_width: 10_000,
            spacing: 300,
            paragraphs: vec![paragraph("C", 1, None)],
            ..Default::default()
        }),
        ..Default::default()
    };
    child.common.width = 20_000;
    child.common.height = 2400;
    child.common.flow_with_text = true;
    child.common.text_wrap = TextWrap::TopAndBottom;
    child.common.vert_rel_to = VertRelTo::Para;
    child.rebuild_grid();
    let host = Paragraph {
        char_count: 9,
        controls: vec![Control::Table(Box::new(child))],
        ..Default::default()
    };
    let mut parent = Table {
        row_count: 1,
        // 두 열 이하 표의 기존 reset 완화와 투명 1×1 해체를 피한다.
        // 이 회귀는 실제 중첩 원장의 경계가 부모 컷으로 전파되는지를 잠근다.
        col_count: 3,
        page_break: TablePageBreak::RowBreak,
        cells: vec![
            Cell {
                width: 20_000,
                paragraphs: vec![host, paragraph("F", 1, Some(false))],
                ..cell(0, 0, Vec::new())
            },
            Cell {
                width: 5000,
                ..cell(0, 1, vec![paragraph("G", 1, None)])
            },
            Cell {
                width: 5000,
                ..cell(0, 2, vec![paragraph("H", 1, None)])
            },
        ],
        ..Default::default()
    };
    parent.common.width = 30_000;
    parent.common.height = 1200;
    parent.common.flow_with_text = true;
    parent.common.text_wrap = TextWrap::TopAndBottom;
    parent.common.vert_rel_to = VertRelTo::Para;
    parent.rebuild_grid();
    let mut core = DocumentCore::new_empty();
    let mut document = core.document().clone();
    document.doc_info.para_shapes = vec![ParaShape::default()];
    let mut section = Section::default();
    section.section_def.page_def = PageDef {
        width: 33_000,
        height: body_height + 1200,
        margin_left: 600,
        margin_right: 600,
        margin_top: 600,
        margin_bottom: 600,
        ..Default::default()
    };
    section.paragraphs = vec![
        // 표 앞 정상 본문 한 줄로 첫 조각 예산을 48px로 만든다.
        // 큰 빈 공간의 작은 로컬 reset을 흡수하는 기존 정책과 분리한다.
        paragraph("I", 1, None),
        Paragraph {
            char_count: 9,
            controls: vec![Control::Table(Box::new(parent))],
            ..Default::default()
        },
    ];
    document.sections = vec![section];
    core.set_document(document);
    core
}

#[derive(Debug)]
struct Glyph {
    page: u32,
    bbox: BoundingBox,
}

fn glyphs(core: &DocumentCore) -> BTreeMap<char, Glyph> {
    fn walk(
        node: &RenderNode,
        page: u32,
        paper_height: f64,
        parent_cell: Option<BoundingBox>,
        out: &mut BTreeMap<char, Glyph>,
    ) {
        let parent_cell = match &node.node_type {
            RenderNodeType::TableCell(cell) if cell.col_span == 1 && cell.row_span == 1 => {
                parent_cell.or(Some(node.bbox))
            }
            _ => parent_cell,
        };
        if let RenderNodeType::TextRun(run) = &node.node_type {
            for ch in run.text.chars().filter(|ch| !ch.is_whitespace()) {
                assert!(
                    out.insert(
                        ch,
                        Glyph {
                            page,
                            bbox: node.bbox
                        }
                    )
                    .is_none(),
                    "원본 글자 {ch:?}가 여러 조각에 반복됐다"
                );
                assert!(
                    node.bbox.y >= -0.5 && node.bbox.y + node.bbox.height <= paper_height + 0.5,
                    "{ch:?}가 종이 밖에 있다: page={page}, bbox={:?}, paper_height={paper_height}",
                    node.bbox
                );
                if let Some(parent) = parent_cell {
                    assert!(
                        node.bbox.y >= parent.y - 0.5
                            && node.bbox.y + node.bbox.height <= parent.y + parent.height + 0.5,
                        "{ch:?}가 부모 셀 밖에 있다: page={page}, glyph={:?}, parent={parent:?}",
                        node.bbox
                    );
                }
            }
        }
        for child in &node.children {
            walk(child, page, paper_height, parent_cell, out);
        }
    }
    let mut result = BTreeMap::new();
    let mut native_text = String::new();
    for page in 0..core.page_count() {
        let tree = core.build_page_render_tree(page).expect("공개 쪽 조판");
        walk(&tree.root, page, tree.root.bbox.height, None, &mut result);
        let layout: Value = serde_json::from_str(
            &core
                .get_page_text_layout_native(page)
                .expect("공개 텍스트 좌표"),
        )
        .expect("텍스트 좌표 JSON");
        for run in layout["runs"].as_array().expect("runs") {
            let text = run["text"].as_str().expect("run text");
            native_text.push_str(text);
            for (marker, paragraph_index) in [('A', 0), ('B', 1)] {
                if text.contains(marker) {
                    let path = run["cellPath"].as_array().expect("병합 셀의 모델 경로");
                    assert_eq!(path.len(), 2, "이 회귀는 실제 중첩 표 경로를 검사한다");
                    let owner = path.last().expect("원본 셀 주소");
                    assert_eq!(owner["cellIndex"], 0, "원본 병합 셀 주소");
                    assert_eq!(
                        owner["cellParaIndex"], paragraph_index,
                        "{marker}의 원본 문단 번호를 보존해야 한다"
                    );
                }
            }
        }
    }
    for ch in result.keys() {
        assert_eq!(
            native_text.chars().filter(|actual| actual == ch).count(),
            1,
            "native 편집 좌표도 {ch:?}를 한 번만 소유해야 한다"
        );
    }
    result
}

#[test]
fn late_row_driver_preserves_merged_source_paragraph_reset() {
    let core = fixture(true, None, 4800);
    let actual = glyphs(&core);
    assert_eq!(
        actual.keys().copied().collect::<String>(),
        "0123456789ABFGHIR"
    );
    assert_eq!(actual[&'A'].page, 0, "첫 문단은 첫 조각 소유다");
    assert!(
        actual[&'B'].page > actual[&'A'].page,
        "늦은 행 driver가 병합 셀의 두 번째 저장 문단 경계를 삼켰다: {actual:?}"
    );
}

#[test]
fn nested_microfragments_reserve_top_and_bottom_caption_once() {
    for direction in [CaptionDirection::Top, CaptionDirection::Bottom] {
        let core = fixture(false, Some(direction), 4800);
        let actual = glyphs(&core);
        assert_eq!(
            actual.keys().copied().collect::<String>(),
            "0123456789ABCFGHIR"
        );
        let caption = &actual[&'C'];
        let first = &actual[&'A'];
        let last = &actual[&'9'];
        let following = &actual[&'F'];
        match direction {
            CaptionDirection::Top => {
                assert_eq!(caption.page, first.page, "위 캡션은 첫 조각 소유다");
                assert!(
                    caption.bbox.y + caption.bbox.height + 4.0 <= first.bbox.y + 0.5,
                    "16px 캡션과 4px 간격을 첫 조각에 예약해야 한다: {actual:?}"
                );
            }
            CaptionDirection::Bottom => {
                assert_eq!(caption.page, last.page, "아래 캡션은 마지막 조각 소유다");
                assert!(
                    last.bbox.y + last.bbox.height + 4.0 <= caption.bbox.y + 0.5,
                    "16px 캡션과 4px 간격을 마지막 조각에 예약해야 한다: {actual:?}"
                );
            }
            _ => unreachable!(),
        }
        assert!(
            following.page > caption.page
                || following.bbox.y >= caption.bbox.y + caption.bbox.height - 0.5,
            "캡션 예약이 빠져 뒤 문단이 겹쳤다: {actual:?}"
        );
    }
}

#[test]
fn single_atom_late_driver_keeps_the_whole_block_on_one_page() {
    let mut core = fixture(false, None, 14_400);
    let mut document = core.document().clone();
    let Control::Table(parent) = &mut document.sections[0].paragraphs[1].controls[0] else {
        panic!("본문의 부모 표");
    };
    let Control::Table(child) = &mut parent.cells[0].paragraphs[0].controls[0] else {
        panic!("부모 셀의 중첩 표");
    };
    let mut atom = paragraph("L", 1, None);
    atom.line_segs[0].line_height = 9600;
    atom.line_segs[0].text_height = 9600;
    atom.line_segs[0].baseline_distance = 7680;
    child.cells[2].paragraphs = vec![atom];
    child.cells[2].height = 9600;
    core.set_document(document);

    // 첫 행 16px + 뒤 행 128px + 뒤 문단 16px + 앞 본문 16px = 176px.
    // 단일 atom driver가 microfragment 대상이 아니어도 192px 본문에 맞는다.
    // 앞 행에 블록 전체 144px를 올리고 뒤 행 128px를 다시 올리면 두 쪽이 된다.
    assert_eq!(
        core.page_count(),
        1,
        "fallback이 같은 블록의 뒤 행 높이를 이중 계상했다: {}",
        core.dump_page_items(None)
    );
    let actual = glyphs(&core);
    assert_eq!(actual.keys().copied().collect::<String>(), "ABFGHILR");
    assert!(actual.values().all(|glyph| glyph.page == 0));
    assert!(
        actual[&'F'].bbox.y >= actual[&'L'].bbox.y + actual[&'L'].bbox.height - 0.5,
        "큰 원자 유닛 뒤 문단이 겹쳤다: {actual:?}"
    );
}
