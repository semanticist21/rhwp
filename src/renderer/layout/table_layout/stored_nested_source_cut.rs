//! 상위 중첩 투영에서도 실제 자식 컷이 소유한 원본 재시작을 보존한다.

use crate::model::control::Control;
use crate::model::table::{Cell, Table};

use super::{LayoutEngine, ResolvedStyleSet};

impl LayoutEngine {
    /// 단일 자식 유닛의 시작 컷을 원본 글줄까지 추적한다.
    /// 블록 컷의 인덱스를 셀 유닛으로 해석하거나 로컬 문단 원점을 쪽 경계로 올리지 않는다.
    pub(super) fn stored_nested_source_unit_frame_break(
        &self,
        cell: &Cell,
        table: &Table,
        index: usize,
        styles: &ResolvedStyleSet,
    ) -> bool {
        let profile = self.profile.get();
        if profile.session_edited()
            || !(profile.hwpx_stored_layout() || profile.hwp5_stored_pagination_layout())
            || self
                .render_normalization
                .borrow()
                .table_text_reflowed(table)
        {
            return false;
        }
        let units = self.cell_units(cell, table, styles);
        let Some(unit) = units.get(index) else {
            return false;
        };
        if Self::stored_in_paragraph_unit_is_original_restart(cell, unit) {
            return true;
        }
        let Some(child) = cell.paragraphs.get(unit.para_idx).and_then(|paragraph| {
            if paragraph.stored_text_partition_is_dirty() || paragraph.cell_format_vpos_dirty {
                return None;
            }
            paragraph.controls.iter().find_map(|control| match control {
                Control::Table(child) => Some(child.as_ref()),
                _ => None,
            })
        }) else {
            return false;
        };
        if let Some((row, fragment)) = unit.nested_row.zip(unit.nested_table_fragment.as_ref()) {
            return self.stored_nested_source_cut_frame_break(
                child,
                row,
                &fragment.start_cut,
                fragment.is_block,
                styles,
            );
        }
        if !unit.mixed_nested_recursive {
            return false;
        }
        let Some(cut) = self
            .mixed_nested_split_from_cut(cell, table, styles, index, index + 1, unit.para_idx)
            .and_then(|split| split.recursive_cut)
        else {
            return false;
        };
        self.stored_nested_source_cut_frame_break(
            child,
            cut.start_row,
            &cut.start_cut,
            cut.start_cut_is_block,
            styles,
        )
    }

    fn stored_nested_source_cut_frame_break(
        &self,
        table: &Table,
        row: usize,
        cut: &[usize],
        is_block: bool,
        styles: &ResolvedStyleSet,
    ) -> bool {
        let mut cells = if is_block {
            let block = super::super::table_partial::rowspan_block_range(table, row);
            Self::row_block_cells(table, block.0, block.1)
        } else {
            Self::row_cut_cell_order(table, row)
                .into_iter()
                .filter_map(|index| table.cells.get(index))
                .collect()
        };
        if is_block {
            cells.sort_by_key(|cell| (cell.row, cell.col));
        }
        cells.into_iter().enumerate().any(|(order, cell)| {
            let Some(&start) = cut.get(order) else {
                return false;
            };
            self.stored_nested_source_unit_frame_break(cell, table, start, styles)
        })
    }
}
