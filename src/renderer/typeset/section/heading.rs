//! 구역 문단 처리의 keep_heading_with_following_block 단계. 조건·예약·발행 순서를 유지한다.
use crate::renderer::typeset::{
    paragraph::{fit, metrics, stored_lines},
    ColumnBreakType, ComposedParagraph, PageItem, Paragraph, ResolvedStyleSet, TypesetEngine,
    TypesetState,
};
impl TypesetEngine {
    pub(in crate::renderer::typeset) fn keeps_with_previous(
        &self,
        st: &TypesetState,
        para_idx: usize,
        paragraphs: &[Paragraph],
        styles: &ResolvedStyleSet,
    ) -> bool {
        let Some(previous_idx) = para_idx.checked_sub(1) else {
            return false;
        };
        let previous = &paragraphs[previous_idx];
        let para = &paragraphs[para_idx];
        st.wrap_around_cs < 0
            && !self.paragraph_has_table(previous)
            && !self.paragraph_has_table(para)
            && para.column_type == ColumnBreakType::None
            && !styles
                .para_styles
                .get(para.para_shape_id as usize)
                .is_some_and(|style| style.page_break_before)
            && styles
                .para_styles
                .get(previous.para_shape_id as usize)
                .is_some_and(|style| style.keep_with_next)
            && st.current_items.iter().any(|item| {
                matches!(item,
                    PageItem::FullParagraph { para_index }
                    | PageItem::PartialParagraph { para_index, .. }
                    if *para_index == previous_idx)
            })
    }

    pub(super) fn keep_with_next(
        &self,
        st: &mut TypesetState,
        para_idx: usize,
        paragraphs: &[Paragraph],
        composed: &[ComposedParagraph],
        styles: &ResolvedStyleSet,
    ) {
        let linked = |index: usize| {
            paragraphs.get(index).is_some_and(|para| {
                styles
                    .para_styles
                    .get(para.para_shape_id as usize)
                    .is_some_and(|style| style.keep_with_next)
            })
        };
        if !linked(para_idx) || st.current_items.is_empty() || st.wrap_around_cs >= 0 {
            return;
        }
        let width = st
            .layout
            .column_areas
            .get(st.current_column as usize)
            .map_or(st.layout.body_area.width, |area| area.width);
        let safety = fit::layout_drift_safety_px(paragraphs);
        let available = (st.available_height() - safety).max(0.0);
        let fresh_available = (st.base_available_height() - safety).max(0.0);
        let stored_page_origin = st.vpos_page_base.filter(|_| st.vpos_page_base_stored);
        let stored_origin = stored_page_origin.unwrap_or(0);
        let stored_available = if stored_page_origin.is_some() {
            st.available_height() - st.vpos_col_anchor
        } else {
            st.available_height()
        };
        let stored_bottom = stored_origin.saturating_add(crate::renderer::px_to_hwpunit(
            stored_available.max(0.0),
            self.dpi,
        ));
        let mut stored_chain_end =
            (st.col_count == 1 && !st.vpos_ladder_dirty).then_some(stored_origin);
        let mut required = 0.0;
        let mut index = para_idx;
        loop {
            let Some(para) = paragraphs.get(index) else {
                return;
            };
            if self.paragraph_has_table(para) {
                return;
            }
            let fmt = self.format_paragraph(para, composed.get(index), styles, Some(width));
            // 명시적 쪽·단 경계와 표의 분할은 해당 배치 경로가 소유한다.
            let follows = paragraphs.get(index + 1).is_some_and(|next| {
                next.column_type == ColumnBreakType::None
                    && !self.paragraph_has_table(next)
                    && !styles
                        .para_styles
                        .get(next.para_shape_id as usize)
                        .is_some_and(|style| style.page_break_before)
            });
            let continues = linked(index) && follows;
            let protected_lines = if continues {
                para.line_segs.as_slice()
            } else {
                para.line_segs.get(..1).unwrap_or_default()
            };
            stored_chain_end = stored_chain_end.and_then(|mut end| {
                if protected_lines.is_empty() || para.stored_text_partition_is_dirty() {
                    return None;
                }
                for line in protected_lines {
                    if super::super::is_synthetic_line_seg(line)
                        || line.vertical_pos < end
                        || line.line_height < 0
                    {
                        return None;
                    }
                    end = line.vertical_pos.saturating_add(line.line_height);
                    if end > stored_bottom {
                        return None;
                    }
                }
                Some(end)
            });
            if !continues {
                // 마지막 문단의 첫 줄만 예약한다. 문단 보호까지 강제하지 않는다.
                required += fmt.spacing_before
                    + fmt
                        .line_heights
                        .first()
                        .copied()
                        .unwrap_or(fmt.height_for_fit);
                break;
            }
            let hints = metrics::flow_hints(para, &fmt, paragraphs, index, st.profile, self.dpi);
            required += fmt.flow_advance_height(
                para,
                st.col_count,
                hints.trim_spacing_before_for_flow,
                st.vpos_ladder_dirty
                    || !stored_lines::spacing_trim_restorable(paragraphs, index)
                    || stored_lines::next_boundary_reverts_spacing_trim(
                        st.profile.hwpx_stored_layout() && !st.profile.hwp3_layout(),
                        paragraphs,
                        styles,
                        index,
                        self.dpi,
                    ),
                st.vpos_page_base.is_none() && st.vpos_lazy_base.is_some(),
            );
            // 한 쪽보다 큰 연결은 기존 줄 분할로 전진한다. 빈 쪽을 재시도하지 않는다.
            if required > fresh_available {
                return;
            }
            index += 1;
        }
        // 저장된 연결이 같은 쪽에 들어가면 높이 보정 전의 누적값으로 옮기지 않는다.
        let stored_chain_fits = stored_chain_end.is_some()
            && (stored_page_origin.is_some()
                || (super::super::saved_flow_marks_page_last(paragraphs, index)
                    && super::super::single_line_visible_bounds_px(
                        &paragraphs[index],
                        stored_origin,
                        self.dpi,
                    )
                    .is_some_and(|bounds| {
                        super::super::saved_bounds_fit_at_flow_tail(
                            bounds,
                            st.current_height,
                            st.available_height(),
                            0.0,
                        )
                    })));
        if stored_chain_fits {
            return;
        }
        if index > para_idx
            && required <= fresh_available
            && st.current_height + required > available
        {
            st.advance_column_or_new_page();
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn keep_heading_with_following_block(
        &self,
        st: &mut TypesetState,
        para_idx: usize,
        para: &Paragraph,
        paragraphs: &[Paragraph],
    ) {
        // [Task #404] heading-orphan 패턴 보정 (vpos 기반).
        // 현재 paragraph 가 누적 height 로는 fit 하지만 HWP vpos 기준 페이지 한계를
        // 넘고, 다음 substantial block(Table/Shape/큰 paragraph)이 잔여 영역에 들어
        // 가지 않을 때 → 현재 paragraph 를 다음 페이지로 push 하여 heading + 후속
        // 블록을 같은 페이지에 배치.
        //
        // 조건 (모두 AND):
        //   A) current_items 비어있지 않음 (페이지 첫 item 자기참조 회피)
        //   B) 단일 단 + wrap-around zone 아님 (multi-column / wrap 의미 차이 회피)
        //   C) 누적 height 로 fit
        //   D) vpos overflow > 1mm (283 HU)
        //   E) 다음 paragraph 의 height 가 substantial (>30px ≈ 8mm) AND 잔여 영역에
        //      들어가지 않음
        //
        // Stage 1 진단 로그 분석으로 false positive 41건 → 1건(pi=83)으로 축소.
        // page_top_vpos 는 current_items 의 첫 item para_index 를 통해 즉시 계산
        // (TypesetState 필드 추적은 typeset_paragraph 내부 페이지 flush 와 동기 안 됨).
        if !st.current_items.is_empty() && st.wrap_around_cs < 0 && st.col_count == 1 {
            let page_first_para_idx = st.current_items.iter().find_map(|item| match item {
                PageItem::FullParagraph { para_index } => Some(*para_index),
                PageItem::PartialParagraph { para_index, .. } => Some(*para_index),
                PageItem::Table { para_index, .. } => Some(*para_index),
                PageItem::PartialTable { para_index, .. } => Some(*para_index),
                PageItem::Shape { para_index, .. } => Some(*para_index),
                PageItem::EndnoteSeparator { .. } => None,
            });
            let page_top_vpos_opt = page_first_para_idx
                .and_then(|pi| paragraphs.get(pi))
                .and_then(|p| p.line_segs.first())
                .filter(|seg| !super::super::is_synthetic_line_seg(seg))
                .map(|s| s.vertical_pos);
            if let (Some(first_seg), Some(page_top_vpos)) = (
                para.line_segs
                    .first()
                    .filter(|seg| !super::super::is_synthetic_line_seg(seg)),
                page_top_vpos_opt,
            ) {
                // 재조판이 생성한 누적 좌표는 저장된 쪽의 소유 근거가 아니다.
                // 앞 쪽에서 일부 줄을 이미 소비한 문단의 첫 좌표로 쪽 원점을
                // 추측하면, 실제로 들어가는 목록 항목까지 다음 쪽으로 밀린다.
                // 혼합 사다리도 전체 경계의 저장 근거가 없으면 흐름 fit에 맡긴다.
                if para
                    .line_segs
                    .iter()
                    .any(super::super::is_synthetic_line_seg)
                {
                    return;
                }
                let body_h_hu =
                    crate::renderer::px_to_hwpunit(st.layout.body_area.height, self.dpi);
                let para_h_px: f64 = para
                    .line_segs
                    .iter()
                    .map(|s| {
                        crate::renderer::hwpunit_to_px(
                            s.line_height.saturating_add(s.line_spacing),
                            self.dpi,
                        )
                    })
                    .sum();
                let para_h_hu = crate::renderer::px_to_hwpunit(para_h_px, self.dpi);
                // [Task #643] vpos_end 는 마지막 줄의 bottom (vpos + lh) 기준.
                // para_h_px 누적은 트레일링 line_spacing 까지 포함하여 ~10-12 HU 과대.
                // HWP 가 페이지 끝에서 트레일링 ls 를 고려하지 않고 lh 만 fit 검사하는
                // 시멘틱 정합 (pi=39 page 3 fits 케이스).
                // 손상 입력의 거대한 vpos/height 로 i32 덧셈이 오버플로(패닉)하지
                // 않도록 saturating — 정상값에선 동일, 손상값은 i32::MAX 로 포화.
                let vpos_end = para
                    .line_segs
                    .last()
                    .map(|s| s.vertical_pos.saturating_add(s.line_height))
                    .unwrap_or(first_seg.vertical_pos.saturating_add(para_h_hu));
                let page_bottom_vpos = page_top_vpos.saturating_add(body_h_hu);

                let avail = st.available_height();
                let current_fits = st.current_height + para_h_px <= avail;
                let vpos_overflow = vpos_end > page_bottom_vpos + 283; // 1mm 안전여유

                let next_h_px: f64 = paragraphs
                    .get(para_idx + 1)
                    .map(|p| {
                        p.line_segs
                            .iter()
                            .map(|s| {
                                crate::renderer::hwpunit_to_px(
                                    s.line_height.saturating_add(s.line_spacing),
                                    self.dpi,
                                )
                            })
                            .sum::<f64>()
                    })
                    .unwrap_or(0.0);
                let next_substantial = next_h_px > 30.0;
                let next_doesnt_fit = st.current_height + para_h_px + next_h_px > avail;

                if current_fits && vpos_overflow && next_substantial && next_doesnt_fit {
                    st.advance_column_or_new_page();
                }
            }
        }
    }
}
