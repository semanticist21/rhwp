//! 셀 경로로 가리킨 표(칸 안의 표·머리말/꼬리말 표)의 구조 편집과 칸 문단 스타일.
//!
//! 평면 API(`insertTableRow`·`mergeTableCells` …)는 `(구역, 문단, 컨트롤)`로 본문 문단의 표만
//! 짚는다. 칸 안 표의 칸에서 부르면 바깥 표가 바뀐다(upstream #8138). 여기의 경로판은 다른
//! `*ByPath` API와 같은 `(control, cell, cell_para)` 사슬을 받고, 마지막 마디의 컨트롤이 고칠
//! 표다. 첫 마디가 머리말·꼬리말(`cell` 0, `cell_para`는 머리말·꼬리말 문단)이어도 된다.
//! 깊이 1 경로는 평면 API에 그대로 맡긴다.

use crate::document_core::helpers::json_ok_with;
use crate::document_core::DocumentCore;
use crate::error::HwpError;
use crate::model::border_paint::PaintRef;
use crate::model::event::DocumentEvent;
use crate::model::table::Table;

/// 칸을 옮기는 편집(합치기·나누기) 뒤 어느 칸을 다시 짤지.
enum Reflow {
    /// 줄·칸 넣기·지우기: 남은 칸의 폭이 그대로라 다시 짜지 않는다(평면 API와 같다).
    None,
    /// 합친 칸: 여러 칸의 문단이 한 칸으로 모여 새 폭과 새 사다리가 필요하다.
    Cell(u16, u16),
    /// 나눈 칸: 저장 줄 폭이 칸 폭을 넘는 칸만 다시 짠다(`reflow_stale_cells_after_split`).
    Stale,
}

impl DocumentCore {
    /// 경로의 표를 `edit`로 고치고 다시 짠다. 칸 모양 테두리 원본(PDF 칸 테두리)이 있는
    /// 표의 합치기·나누기는 평면 API처럼 거절한다.
    fn edit_table_by_path(
        &mut self,
        section_idx: usize,
        parent_para_idx: usize,
        path: &[(usize, usize, usize)],
        topology: bool,
        reflow: Reflow,
        edit: impl FnOnce(&mut Table) -> Result<(), String>,
    ) -> Result<(u16, u16, usize), HwpError> {
        let table = self.resolve_table_mut_by_cell_path(section_idx, parent_para_idx, path)?;
        if topology
            && table
                .cells
                .iter()
                .flat_map(|c| &c.paragraphs)
                .flat_map(|p| &p.controls)
                .filter_map(PaintRef::from_control)
                .any(|p| p.common().cell_border_paint.is_some())
        {
            return Err(HwpError::RenderError(
                "이 표의 원본 테두리를 보존하는 셀 병합·나누기는 아직 지원하지 않습니다".into(),
            ));
        }
        edit(table).map_err(HwpError::RenderError)?;
        let counts = (table.row_count, table.col_count, table.cells.len());
        let cells: Vec<(usize, usize)> = match reflow {
            Reflow::None => Vec::new(),
            Reflow::Cell(row, col) => table
                .cells
                .iter()
                .position(|c| c.row == row && c.col == col)
                .map(|i| vec![(i, table.cells[i].paragraphs.len())])
                .unwrap_or_default(),
            Reflow::Stale => {
                let metrics = Self::table_cell_reflow_metrics(table);
                table
                    .cells
                    .iter()
                    .enumerate()
                    .filter(|(i, cell)| {
                        metrics.get(*i).is_some_and(|&(width, _, _)| {
                            cell.paragraphs
                                .iter()
                                .flat_map(|p| &p.line_segs)
                                .any(|seg| i64::from(seg.segment_width) > i64::from(width))
                        })
                    })
                    .map(|(i, cell)| (i, cell.paragraphs.len()))
                    .collect()
            }
        };
        let depth = path.len() - 1;
        let control = path[depth].0;
        let mut cell_path = path.to_vec();
        for (cell, count) in cells {
            for para in 0..count {
                cell_path[depth] = (control, cell, para);
                self.reflow_cell_paragraph_by_path(section_idx, parent_para_idx, &cell_path, para);
            }
            cell_path[depth] = (control, cell, 0);
            self.rebuild_cell_vpos_ladder_by_path(section_idx, parent_para_idx, &cell_path);
        }
        // 표를 담은 칸(머리말·꼬리말) 문단의 줄 높이와 뒤 문단 위치도 표 크기를 따른다.
        let container = &path[..depth];
        let host = container[depth - 1].2;
        self.reflow_cell_paragraph_by_path(section_idx, parent_para_idx, container, host);
        self.recalculate_cell_paragraph_vpos_by_path(
            section_idx,
            parent_para_idx,
            container,
            host,
            None,
        );
        self.mark_cell_control_dirty(section_idx, parent_para_idx, path[0].0);
        self.document.sections[section_idx].raw_stream = None;
        self.recompose_section(section_idx);
        self.paginate_if_needed();
        self.invalidate_page_tree_cache();
        Ok(counts)
    }

    fn table_path_event(&mut self, section: usize, para: usize, path: &[(usize, usize, usize)]) {
        self.event_log.push(DocumentEvent::CellTextChanged {
            section,
            para,
            ctrl: path[0].0,
            cell: path[0].1,
        });
    }

    /// 경로의 표에 줄(`row`)·칸(`column`)을 넣거나 지운다. `after`는 아래·오른쪽에 넣기다.
    /// 반환: `{"ok":true,"rowCount":..,"colCount":..}` (평면 API와 같다).
    pub fn edit_table_lines_by_path(
        &mut self,
        section_idx: usize,
        parent_para_idx: usize,
        path: &[(usize, usize, usize)],
        row: bool,
        delete: bool,
        index: u16,
        after: bool,
    ) -> Result<String, HwpError> {
        let Some(&(control_idx, _, _)) = path.first() else {
            return Err(HwpError::RenderError("경로가 비어있습니다".into()));
        };
        if path.len() == 1 {
            let (s, p, c) = (section_idx, parent_para_idx, control_idx);
            return match (row, delete) {
                (true, false) => self.insert_table_row_native(s, p, c, index, after),
                (false, false) => self.insert_table_column_native(s, p, c, index, after),
                (true, true) => self.delete_table_row_native(s, p, c, index),
                (false, true) => self.delete_table_column_native(s, p, c, index),
            };
        }
        let (rows, cols, _) = self.edit_table_by_path(
            section_idx,
            parent_para_idx,
            path,
            false,
            Reflow::None,
            |table| match (row, delete) {
                (true, false) => table.insert_row_inheriting(index, after, |_, _| {}),
                (false, false) => table.insert_column_inheriting(index, after, |_, _| {}),
                (true, true) => table.delete_row(index),
                (false, true) => table.delete_column(index),
            },
        )?;
        self.table_path_event(section_idx, parent_para_idx, path);
        Ok(json_ok_with(&format!(
            "\"rowCount\":{},\"colCount\":{}",
            rows, cols
        )))
    }

    /// 경로의 표에서 `(start_row, start_col)`~`(end_row, end_col)` 칸을 합친다.
    /// 반환: `{"ok":true,"cellCount":..}`.
    #[allow(clippy::too_many_arguments)]
    pub fn merge_table_cells_by_path(
        &mut self,
        section_idx: usize,
        parent_para_idx: usize,
        path: &[(usize, usize, usize)],
        start_row: u16,
        start_col: u16,
        end_row: u16,
        end_col: u16,
    ) -> Result<String, HwpError> {
        let Some(&(control_idx, _, _)) = path.first() else {
            return Err(HwpError::RenderError("경로가 비어있습니다".into()));
        };
        if path.len() == 1 {
            return self.merge_table_cells_native(
                section_idx,
                parent_para_idx,
                control_idx,
                start_row,
                start_col,
                end_row,
                end_col,
            );
        }
        let (_, _, cells) = self.edit_table_by_path(
            section_idx,
            parent_para_idx,
            path,
            true,
            Reflow::Cell(start_row, start_col),
            |table| table.merge_cells(start_row, start_col, end_row, end_col),
        )?;
        self.table_path_event(section_idx, parent_para_idx, path);
        Ok(json_ok_with(&format!("\"cellCount\":{}", cells)))
    }

    /// 경로의 표에서 칸 하나(`range`가 한 칸)나 칸 범위를 `n_rows`줄 × `m_cols`칸으로 나눈다.
    /// `merge_first`는 한 칸 나누기에서 병합 칸을 먼저 푼다(`splitTableCellInto`와 같다).
    /// 반환: `{"ok":true,"cellCount":..}`.
    #[allow(clippy::too_many_arguments)]
    pub fn split_table_cells_by_path(
        &mut self,
        section_idx: usize,
        parent_para_idx: usize,
        path: &[(usize, usize, usize)],
        range: (u16, u16, u16, u16),
        n_rows: u16,
        m_cols: u16,
        equal_row_height: bool,
        merge_first: bool,
    ) -> Result<String, HwpError> {
        let Some(&(control_idx, _, _)) = path.first() else {
            return Err(HwpError::RenderError("경로가 비어있습니다".into()));
        };
        let (start_row, start_col, end_row, end_col) = range;
        let single = start_row == end_row && start_col == end_col;
        if path.len() == 1 {
            return if single {
                self.split_table_cell_into_native(
                    section_idx,
                    parent_para_idx,
                    control_idx,
                    start_row,
                    start_col,
                    n_rows,
                    m_cols,
                    equal_row_height,
                    merge_first,
                )
            } else {
                self.split_table_cells_in_range_native(
                    section_idx,
                    parent_para_idx,
                    control_idx,
                    start_row,
                    start_col,
                    end_row,
                    end_col,
                    n_rows,
                    m_cols,
                    equal_row_height,
                )
            };
        }
        let (_, _, cells) = self.edit_table_by_path(
            section_idx,
            parent_para_idx,
            path,
            n_rows != 1 || m_cols != 1,
            Reflow::Stale,
            |table| {
                if single {
                    table.split_cell_into(
                        start_row,
                        start_col,
                        n_rows,
                        m_cols,
                        equal_row_height,
                        merge_first,
                    )
                } else {
                    table.split_cells_in_range(
                        start_row,
                        start_col,
                        end_row,
                        end_col,
                        n_rows,
                        m_cols,
                        equal_row_height,
                    )
                }
            },
        )?;
        self.table_path_event(section_idx, parent_para_idx, path);
        Ok(json_ok_with(&format!("\"cellCount\":{}", cells)))
    }

    /// 경로의 칸(머리말·꼬리말) 문단 스타일 번호.
    pub fn get_cell_style_at_by_path(
        &self,
        section_idx: usize,
        parent_para_idx: usize,
        path: &[(usize, usize, usize)],
    ) -> Result<usize, HwpError> {
        Ok(self
            .resolve_paragraph_by_path(section_idx, parent_para_idx, path)?
            .style_id as usize)
    }

    /// 경로의 칸(머리말·꼬리말) 문단에 스타일을 준다. 문단 스타일은 `apply_cell_style_native`처럼
    /// 직접 고친 문단 모양을 지키고 스타일 글자 모양을 바꾸며, 글자 스타일은 문단 글 전체에 준다.
    pub fn apply_cell_style_by_path(
        &mut self,
        section_idx: usize,
        parent_para_idx: usize,
        path: &[(usize, usize, usize)],
        style_id: usize,
    ) -> Result<String, HwpError> {
        let Some(&(control_idx, cell_idx, cell_para_idx)) = path.first() else {
            return Err(HwpError::RenderError("경로가 비어있습니다".into()));
        };
        if path.len() == 1
            && !self.is_header_footer_control(section_idx, parent_para_idx, control_idx)
        {
            return self.apply_cell_style_native(
                section_idx,
                parent_para_idx,
                control_idx,
                cell_idx,
                cell_para_idx,
                style_id,
            );
        }
        let style = self
            .document
            .doc_info
            .styles
            .get(style_id)
            .cloned()
            .ok_or_else(|| HwpError::RenderError(format!("스타일 {} 범위 초과", style_id)))?;
        let char_shape = style.char_shape_id as u32;
        let (current_style, current_shape) = {
            let para = self.resolve_paragraph_by_path(section_idx, parent_para_idx, path)?;
            (para.style_id, para.para_shape_id)
        };
        let old_style = self
            .document
            .doc_info
            .styles
            .get(current_style as usize)
            .cloned();
        if style.style_type == 1 {
            let para = self.get_cell_paragraph_mut_by_path(section_idx, parent_para_idx, path)?;
            para.apply_char_shape_to_entire_text(char_shape);
            let len = para.text.chars().count();
            para.set_char_style_range(0, len, Some(style_id as u32));
            para.invalidate_layout_inputs();
        } else {
            let para_shape = match old_style.as_ref() {
                Some(old) if current_shape != old.para_shape_id => current_shape,
                _ => self.resolve_style_para_shape_id(style_id, current_shape),
            };
            // 문단 모양 번호는 경로판 setter가 바꾸며 다시 짠다(줄 원점·번호 표지 포함).
            self.set_cell_para_shape_id_by_path(section_idx, parent_para_idx, path, para_shape)?;
            let para = self.get_cell_paragraph_mut_by_path(section_idx, parent_para_idx, path)?;
            para.style_id = style_id as u8;
            match old_style {
                Some(old) => para.replace_style_char_shape_preserving_overrides(
                    old.char_shape_id as u32,
                    char_shape,
                ),
                None => para.set_single_char_shape(char_shape),
            }
            para.invalidate_layout_inputs();
        }
        let inner = path[path.len() - 1].2;
        self.reflow_cell_paragraph_by_path(section_idx, parent_para_idx, path, inner);
        self.recalculate_cell_paragraph_vpos_by_path(
            section_idx,
            parent_para_idx,
            path,
            inner,
            None,
        );
        self.mark_cell_control_dirty(section_idx, parent_para_idx, control_idx);
        self.document.sections[section_idx].raw_stream = None;
        self.rebuild_section_deferred_in_batch(section_idx);
        self.event_log.push(DocumentEvent::ParaFormatChanged {
            section: section_idx,
            para: parent_para_idx,
        });
        Ok("{\"ok\":true}".into())
    }
}
