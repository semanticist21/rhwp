//! 셀 경로로 가리킨 표(칸 안의 표·머리말/꼬리말 표)의 구조 편집과 칸 문단 스타일.
//!
//! 평면 API(`insertTableRow`·`mergeTableCells` …)는 `(구역, 문단, 컨트롤)`로 본문 문단의 표만
//! 짚는다. 칸 안 표의 칸에서 부르면 바깥 표가 바뀐다(upstream #8138). 여기의 경로판은 다른
//! `*ByPath` API와 같은 `(control, cell, cell_para)` 사슬을 받고, 마지막 마디의 컨트롤이 고칠
//! 표다. 첫 마디가 머리말·꼬리말(`cell` 0, `cell_para`는 머리말·꼬리말 문단)이어도 된다.
//! 깊이 1 경로는 평면 API에 그대로 맡긴다.

use crate::document_core::helpers::json_ok_with;
use crate::document_core::{DocumentCore, TableTransposeClipboard};
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
    /// 칸 크기를 맞춘 표: 모든 칸을 다시 짠다.
    /// ponytail: 칸 안 표는 작아 모두 다시 짠다. 큰 표가 느리면 넓어진 칸만 받는다.
    All,
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
            Reflow::All => table
                .cells
                .iter()
                .enumerate()
                .map(|(i, cell)| (i, cell.paragraphs.len()))
                .collect(),
        };
        self.relayout_table_cells_by_path(section_idx, parent_para_idx, path, &cells);
        Ok(counts)
    }

    /// 경로 표의 `cells`(칸 번호, 문단 수)를 다시 짜고 칸마다 줄 위치를 처음부터 쌓은 뒤, 표를 담은
    /// 칸 문단과 구역을 다시 조판한다. 깊이 2 이상 경로만 받는다.
    pub(crate) fn relayout_table_cells_by_path(
        &mut self,
        section_idx: usize,
        parent_para_idx: usize,
        path: &[(usize, usize, usize)],
        cells: &[(usize, usize)],
    ) {
        let depth = path.len() - 1;
        let control = path[depth].0;
        let mut cell_path = path.to_vec();
        for &(cell, count) in cells {
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

    /// 경로의 칸 블록 너비·높이를 같게 맞춘다(`equalizeTableCellSizes`의 경로판). 깊이 1이면 평면
    /// API다. 칸 안 표의 높이는 그려진 칸 상자(`getTableCellBboxesByPath`)로 잰 줄 높이를 쓴다.
    pub fn equalize_table_cell_sizes_by_path(
        &mut self,
        section_idx: usize,
        parent_para_idx: usize,
        path: &[(usize, usize, usize)],
        block: (u16, u16, u16, u16),
        axis: crate::model::table::CellSizeAxis,
    ) -> Result<String, HwpError> {
        use crate::model::table::CellSizeAxis;
        let Some(&(control_idx, _, _)) = path.first() else {
            return Err(HwpError::RenderError("경로가 비어있습니다".into()));
        };
        if path.len() == 1 {
            return self.equalize_table_cell_sizes_native(
                section_idx,
                parent_para_idx,
                control_idx,
                block,
                axis,
            );
        }
        let bounds = match axis {
            CellSizeAxis::Width => self
                .resolve_table_by_path(section_idx, parent_para_idx, path)?
                .column_bounds()
                .ok_or_else(|| {
                    HwpError::RenderError(
                        "행마다 칸 경계가 달라 셀 너비를 같게 맞출 수 없습니다".into(),
                    )
                })?,
            CellSizeAxis::Height => {
                self.drawn_row_bounds_by_path(section_idx, parent_para_idx, path)?
            }
        };
        let mut moved_to = Vec::new();
        let (rows, cols, _) = self.edit_table_by_path(
            section_idx,
            parent_para_idx,
            path,
            true,
            Reflow::All,
            |table| {
                moved_to = table.equalize_cell_block(axis, block, &bounds)?.0;
                Ok(())
            },
        )?;
        // 재측정이 바깥 표의 직전 행 배분을 하한으로 쓰지 않게 버린다(평면 API와 같다).
        if let Some(measured) = self.measured_sections.get_mut(section_idx) {
            measured
                .tables
                .retain(|t| t.para_index != parent_para_idx || t.control_index != control_idx);
        }
        self.recompose_section(section_idx);
        self.paginate_if_needed();
        self.table_path_event(section_idx, parent_para_idx, path);
        let moved_to: Vec<String> = moved_to.iter().map(usize::to_string).collect();
        Ok(json_ok_with(&format!(
            "\"rowCount\":{},\"colCount\":{},\"cellMap\":[{}]",
            rows,
            cols,
            moved_to.join(",")
        )))
    }

    /// 칸 안 표의 그려진 줄 경계(HWPUNIT, 0부터 누적). 줄마다 그 줄에서 시작하는 칸의 위 끝을 쓴다.
    fn drawn_row_bounds_by_path(
        &self,
        section_idx: usize,
        parent_para_idx: usize,
        path: &[(usize, usize, usize)],
    ) -> Result<Vec<i64>, HwpError> {
        let fail = || {
            HwpError::RenderError("표의 행 높이를 재지 못해 셀 높이를 같게 맞출 수 없습니다".into())
        };
        let row_count = usize::from(
            self.resolve_table_by_path(section_idx, parent_para_idx, path)?
                .row_count,
        );
        let path_json = serde_json::to_string(
            &path
                .iter()
                .map(|&(c, i, p)| serde_json::json!({"controlIndex": c, "cellIndex": i, "cellParaIndex": p}))
                .collect::<Vec<_>>(),
        )
        .map_err(|_| fail())?;
        let boxes: Vec<serde_json::Value> = serde_json::from_str(
            &self.get_table_cell_bboxes_by_path_native(section_idx, parent_para_idx, &path_json)?,
        )
        .map_err(|_| fail())?;
        let num = |b: &serde_json::Value, k: &str| b[k].as_f64();
        let page = boxes.first().and_then(|b| b["pageIndex"].as_u64());
        let mut tops = vec![None; row_count];
        let mut bottom = f64::MIN;
        for b in boxes.iter().filter(|b| b["pageIndex"].as_u64() == page) {
            let (Some(row), Some(y), Some(h)) = (b["row"].as_u64(), num(b, "y"), num(b, "h"))
            else {
                return Err(fail());
            };
            let top = tops.get_mut(row as usize).ok_or_else(fail)?;
            *top = Some(top.map_or(y, |t: f64| t.min(y)));
            bottom = bottom.max(y + h);
        }
        let tops: Vec<f64> = tops.into_iter().collect::<Option<_>>().ok_or_else(fail)?;
        let origin = tops[0];
        let to_hu = |px: f64| ((px - origin) * 7200.0 / self.dpi).round() as i64;
        let mut bounds: Vec<i64> = tops.iter().map(|&t| to_hu(t)).collect();
        bounds.push(to_hu(bottom));
        Ok(bounds)
    }

    /// 경로의 표에서 칸 블록만 남긴 표(한컴 셀 블록 복사)를 만든다.
    fn table_cell_block_by_path(
        &self,
        section_idx: usize,
        parent_para_idx: usize,
        path: &[(usize, usize, usize)],
        (start_row, start_col, end_row, end_col): (u16, u16, u16, u16),
    ) -> Result<Table, HwpError> {
        self.resolve_table_by_path(section_idx, parent_para_idx, path)?
            .cell_block(start_row, start_col, end_row, end_col)
            .map_err(HwpError::RenderError)
    }

    /// 경로 표의 칸 블록을 블록만 남긴 새 표로 내부 클립보드에 복사한다(`copyTableCells`의 경로판).
    /// 반환: `{"ok":true,"text":".."}`.
    pub fn copy_table_cells_by_path(
        &mut self,
        section_idx: usize,
        parent_para_idx: usize,
        path: &[(usize, usize, usize)],
        range: (u16, u16, u16, u16),
    ) -> Result<String, HwpError> {
        let block = self.table_cell_block_by_path(section_idx, parent_para_idx, path, range)?;
        let mut cells: Vec<_> = block.cells.iter().collect();
        cells.sort_by_key(|cell| (cell.row, cell.col));
        let mut text = String::new();
        for (index, cell) in cells.iter().enumerate() {
            if index > 0 {
                text.push(if cells[index - 1].row == cell.row {
                    '\t'
                } else {
                    '\n'
                });
            }
            let paragraphs: Vec<&str> = cell.paragraphs.iter().map(|p| p.text.as_str()).collect();
            text.push_str(&paragraphs.join(" "));
        }
        // 클립보드 문단(글자 모양·문단 모양·컨트롤 자료)은 표 복사와 같게 만들고 표만 블록으로 바꾼다.
        let depth = path.len() - 1;
        self.copy_control_native(section_idx, parent_para_idx, &path[..depth], path[depth].0)?;
        if let Some(clipboard) = self.clipboard.as_mut() {
            clipboard.paragraphs[0].controls[0] =
                crate::model::control::Control::Table(Box::new(block));
            clipboard.plain_text = text.clone();
        }
        Ok(json_ok_with(&format!(
            "\"text\":\"{}\"",
            crate::document_core::helpers::json_escape(&text)
        )))
    }

    /// 경로 표의 칸 블록을 블록만 남긴 표의 HTML로 내보낸다(`exportTableCellsHtml`의 경로판).
    pub fn export_table_cells_html_by_path(
        &self,
        section_idx: usize,
        parent_para_idx: usize,
        path: &[(usize, usize, usize)],
        range: (u16, u16, u16, u16),
    ) -> Result<String, HwpError> {
        let block = crate::model::control::Control::Table(Box::new(
            self.table_cell_block_by_path(section_idx, parent_para_idx, path, range)?,
        ));
        let mut html = String::from("<html><body>\n<!--StartFragment-->\n");
        html.push_str(&self.control_to_html(&block, 0));
        html.push_str("<!--EndFragment-->\n</body></html>");
        crate::document_core::html_table_import::validate_html_common(&html)?;
        Ok(html)
    }

    /// 경로 표의 칸 범위를 행/열 바꿈 복사 버퍼에 담는다(`copyTableCellsTransposed`의 경로판).
    pub fn copy_table_cells_transposed_by_path(
        &mut self,
        section_idx: usize,
        parent_para_idx: usize,
        path: &[(usize, usize, usize)],
        (start_row, start_col, end_row, end_col): (u16, u16, u16, u16),
    ) -> Result<String, HwpError> {
        let data = self
            .resolve_table_mut_by_cell_path(section_idx, parent_para_idx, path)?
            .copy_transpose_range(start_row, start_col, end_row, end_col)
            .map_err(HwpError::RenderError)?;
        let (rows, cols) = (data.source_rows, data.source_cols);
        self.table_transpose_clipboard = Some(TableTransposeClipboard { data });
        Ok(transpose_json(rows, cols))
    }

    /// 행/열 바꿈 복사 버퍼를 경로 표의 (`start_row`, `start_col`)부터 붙인다(`pasteTableCellsTransposed`).
    pub fn paste_table_cells_transposed_by_path(
        &mut self,
        section_idx: usize,
        parent_para_idx: usize,
        path: &[(usize, usize, usize)],
        start_row: u16,
        start_col: u16,
    ) -> Result<String, HwpError> {
        let Some(&(control_idx, _, _)) = path.first() else {
            return Err(HwpError::RenderError("경로가 비어있습니다".into()));
        };
        if path.len() == 1 {
            return self.paste_table_cells_transposed_native(
                section_idx,
                parent_para_idx,
                control_idx,
                start_row,
                start_col,
            );
        }
        let data = self
            .table_transpose_clipboard
            .as_ref()
            .ok_or_else(|| HwpError::RenderError("행/열 바꿈 복사 데이터가 없습니다".into()))?
            .data
            .clone();
        let changed = self
            .resolve_table_mut_by_cell_path(section_idx, parent_para_idx, path)?
            .paste_transposed_cells(start_row, start_col, &data)
            .map_err(HwpError::RenderError)?;
        self.relayout_table_cells_by_path(section_idx, parent_para_idx, path, &changed);
        self.table_path_event(section_idx, parent_para_idx, path);
        Ok(transpose_json(data.source_rows, data.source_cols))
    }

    /// 경로 표 전체의 행과 열을 제자리에서 바꾼다(`transposeTableCellsInPlace`의 경로판).
    pub fn transpose_table_cells_in_place_by_path(
        &mut self,
        section_idx: usize,
        parent_para_idx: usize,
        path: &[(usize, usize, usize)],
    ) -> Result<String, HwpError> {
        let Some(&(control_idx, _, _)) = path.first() else {
            return Err(HwpError::RenderError("경로가 비어있습니다".into()));
        };
        if path.len() == 1 {
            return self.transpose_table_cells_in_place_native(
                section_idx,
                parent_para_idx,
                control_idx,
            );
        }
        let table = self.resolve_table_mut_by_cell_path(section_idx, parent_para_idx, path)?;
        let (rows, cols) = (table.row_count, table.col_count);
        let changed = table
            .transpose_table_in_place()
            .map_err(HwpError::RenderError)?;
        self.relayout_table_cells_by_path(section_idx, parent_para_idx, path, &changed);
        self.table_path_event(section_idx, parent_para_idx, path);
        Ok(transpose_json(rows, cols))
    }
}

fn transpose_json(source_rows: u16, source_cols: u16) -> String {
    json_ok_with(&format!(
        "\"sourceRows\":{},\"sourceCols\":{},\"targetRows\":{},\"targetCols\":{}",
        source_rows, source_cols, source_cols, source_rows
    ))
}
