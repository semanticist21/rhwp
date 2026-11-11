//! HWP 저장의 탭 폭. 한컴은 HWP 탭 확장 자료의 폭을 다시 계산하지 않고 그대로 쓴다.
//! 편집으로 친 탭은 폭 0 자리표로 남으므로, 저장 스냅숏에 조판한 폭과 종류를 적는다.
//! (HWPX는 한컴이 탭 폭을 다시 계산해 자리표 그대로 둔다.)

use std::collections::{BTreeMap, BTreeSet};

use crate::document_core::DocumentCore;
use crate::model::control::Control;
use crate::model::document::Document;
use crate::model::paragraph::{tab_ext_is_placeholder, Paragraph, TAB_EXT_PLACEHOLDER};
use crate::renderer::layout::find_next_tab_stop;
use crate::renderer::pagination::PageItem;
use crate::renderer::render_tree::{RenderNode, RenderNodeType, TextRunNode};

/// (구역, 최상위 문단, 머리말·꼬리말(컨트롤, 그 안 문단), 셀·글상자 경로(컨트롤, 셀, 셀 문단))
type ParaKey = (
    usize,
    usize,
    Option<(usize, usize)>,
    Vec<(usize, usize, usize)>,
);

/// 폭이 정해지지 않은 탭이 있는 문단. 셀·글상자·머리말·꼬리말 안까지 본다.
fn has_unlaid_tab(para: &Paragraph) -> bool {
    let own = para
        .text
        .chars()
        .filter(|&ch| ch == '\t')
        .enumerate()
        .any(|(k, _)| para.tab_extended.get(k).is_none_or(tab_ext_is_placeholder));
    own || para.controls.iter().any(|control| match control {
        Control::Table(table) => table
            .cells
            .iter()
            .any(|cell| cell.paragraphs.iter().any(has_unlaid_tab)),
        Control::Shape(shape) => (0..)
            .map_while(|index| crate::document_core::helpers::get_textbox_at(shape, index))
            .any(|text_box| text_box.paragraphs.iter().any(has_unlaid_tab)),
        Control::Header(header) => header.paragraphs.iter().any(has_unlaid_tab),
        Control::Footer(footer) => footer.paragraphs.iter().any(has_unlaid_tab),
        _ => false,
    })
}

fn paragraph_mut<'a>(doc: &'a mut Document, key: &ParaKey) -> Option<&'a mut Paragraph> {
    let (sec, parent, header_footer, path) = key;
    let mut para = doc.sections.get_mut(*sec)?.paragraphs.get_mut(*parent)?;
    if let Some((control, index)) = *header_footer {
        para = match para.controls.get_mut(control)? {
            Control::Header(header) => header.paragraphs.get_mut(index)?,
            Control::Footer(footer) => footer.paragraphs.get_mut(index)?,
            _ => return None,
        };
    }
    for &(control, cell, cell_para) in path {
        para = match para.controls.get_mut(control)? {
            Control::Table(table) => table.cells.get_mut(cell)?.paragraphs.get_mut(cell_para)?,
            Control::Shape(shape) => {
                crate::document_core::helpers::get_textbox_at_mut(shape, cell)?
                    .paragraphs
                    .get_mut(cell_para)?
            }
            _ => return None,
        };
    }
    Some(para)
}

/// 문단 글자 번호의 탭마다 적을 확장 자료.
type FoundTabs = BTreeMap<ParaKey, BTreeMap<usize, [u16; 7]>>;

impl DocumentCore {
    /// 저장 스냅숏에 적을 탭 폭이 있는지(빠른 저장 경로 판정용).
    pub(crate) fn has_unlaid_tabs(&self) -> bool {
        self.document
            .sections
            .iter()
            .any(|section| section.paragraphs.iter().any(has_unlaid_tab))
    }

    /// 자리표 탭에 화면 조판의 폭·종류를 적는다. 본문·표 셀·글상자와 머리말·꼬리말 문단을 다룬다.
    // ponytail: 각주·미주와 머리말 안 표의 탭은 자리표로 남는다. 한컴에서 어긋나면 그 렌더 트리도 걷는다.
    pub(crate) fn write_laid_out_tab_widths(&self, snapshot: &mut Document) {
        let targets: BTreeSet<(usize, usize)> = self
            .document
            .sections
            .iter()
            .enumerate()
            .flat_map(|(sec, section)| {
                section
                    .paragraphs
                    .iter()
                    .enumerate()
                    .filter(|(_, para)| has_unlaid_tab(para))
                    .map(move |(para, _)| (sec, para))
            })
            .collect();
        if targets.is_empty() {
            return;
        }
        let mut found = FoundTabs::new();
        for page in 0..self.page_count() {
            let Ok((content, _, _)) = self.find_page(page) else {
                continue;
            };
            let sec = content.section_index;
            let in_body = content
                .column_contents
                .iter()
                .flat_map(|column| &column.items)
                .any(|item| match item {
                    PageItem::FullParagraph { para_index, .. }
                    | PageItem::PartialParagraph { para_index, .. }
                    | PageItem::Table { para_index, .. }
                    | PageItem::PartialTable { para_index, .. } => {
                        targets.contains(&(sec, *para_index))
                    }
                    _ => false,
                });
            let in_header_footer = [&content.active_header, &content.active_footer]
                .into_iter()
                .flatten()
                .any(|hf| targets.contains(&(hf.source_section_index, hf.para_index)));
            if !in_body && !in_header_footer {
                continue;
            }
            let Ok(tree) = self.build_page_tree_cached(page) else {
                continue;
            };
            collect_tab_widths(&tree.root, self.dpi, &mut found);
        }
        for (key, tabs) in found {
            let Some(para) = paragraph_mut(snapshot, &key) else {
                continue;
            };
            for (char_index, ext) in tabs {
                let k = para
                    .text
                    .chars()
                    .take(char_index)
                    .filter(|&c| c == '\t')
                    .count();
                if para
                    .tab_extended
                    .get(k)
                    .is_some_and(|stored| !tab_ext_is_placeholder(stored))
                {
                    continue;
                }
                if para.tab_extended.len() <= k {
                    para.tab_extended.resize(k + 1, TAB_EXT_PLACEHOLDER);
                }
                para.tab_extended[k] = ext;
            }
        }
    }
}

fn collect_tab_widths(node: &RenderNode, dpi: f64, found: &mut FoundTabs) {
    // 바탕쪽·각주 영역과 캡션 줄은 문단 주소가 본문 문단과 겹칠 수 있어 건너뛴다.
    match &node.node_type {
        RenderNodeType::MasterPage | RenderNodeType::FootnoteArea => return,
        RenderNodeType::TextLine(line) if line.caption_owner.is_some() => return,
        _ => {}
    }
    if let RenderNodeType::TextRun(run) = &node.node_type {
        collect_run_tab_widths(node, run, dpi, found);
    }
    for child in &node.children {
        collect_tab_widths(child, dpi, found);
    }
}

fn collect_run_tab_widths(node: &RenderNode, run: &TextRunNode, dpi: f64, found: &mut FoundTabs) {
    let (Some(start), Some(sec), Some(para_index)) =
        (run.char_start, run.section_index, run.para_index)
    else {
        return;
    };
    let text = run.display_or_text();
    if !text.contains('\t')
        || run.char_overlap.is_some()
        || text.chars().count() != run.text.chars().count()
    {
        return;
    }
    let key: ParaKey = match (&node.header_footer_source, &run.cell_context) {
        // 머리말·꼬리말의 문단 번호는 그 컨트롤 안 번호다.
        (Some((hf_sec, hf)), None) => (
            *hf_sec,
            hf.outer_para_index,
            Some((hf.outer_control_index, para_index)),
            Vec::new(),
        ),
        (Some(_), Some(_)) => return,
        (None, None) => (sec, para_index, None, Vec::new()),
        (None, Some(cell)) => (
            sec,
            cell.parent_para_index,
            None,
            cell.path
                .iter()
                .map(|entry| (entry.control_index, entry.cell_index, entry.cell_para_index))
                .collect(),
        ),
    };
    let positions = run.replay_positions_for(text);
    let style = &run.style;
    let default_tab = if style.default_tab_width > 0.0 {
        style.default_tab_width
    } else {
        48.0
    };
    for (i, ch) in text.chars().enumerate() {
        if ch != '\t' || i + 1 >= positions.len() {
            continue;
        }
        let width =
            crate::renderer::px_to_hwpunit(positions[i + 1] - positions[i], dpi).max(0) as u32;
        // 조판이 이 탭에 쓴 정지와 같은 정지를 찾아 종류·채움을 적는다.
        let (_, kind, fill) = find_next_tab_stop(
            style.line_x_offset + positions[i],
            &style.tab_stops,
            default_tab,
            style.auto_tab_right,
            style.available_width,
            style.text_start_offset,
        );
        let ext = [
            width as u16,
            (width >> 16) as u16,
            ((u16::from(kind) + 1) << 8) | u16::from(fill),
            0,
            0,
            0,
            0x0009,
        ];
        found.entry(key.clone()).or_default().insert(start + i, ext);
    }
}
