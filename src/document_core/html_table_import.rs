//! HTML 표 파싱 + BorderFill 생성 + 이미지 파싱 관련 native 메서드

use super::helpers::*;
use crate::document_core::DocumentCore;
use crate::model::control::Control;
use crate::model::paragraph::Paragraph;
use crate::model::shape::common_obj_offsets;

// 명시한 한 칸 치수는 유지하고, 병합 칸의 부족분만 미지정 칸부터 나눈다.
fn html_table_tracks(
    count: usize,
    dimensions: &[(usize, usize, Option<u32>)],
    default: u32,
) -> Result<Vec<u32>, crate::error::HwpError> {
    let invalid =
        || crate::error::HwpError::RenderError("표의 치수가 올바르지 않습니다".to_string());
    for &(start, span, _) in dimensions {
        if span == 0 || start.checked_add(span).is_none_or(|end| end > count) {
            return Err(invalid());
        }
    }
    let mut tracks = vec![0u32; count];
    let mut fixed = vec![false; count];
    let mut covered = vec![false; count];
    for &(start, span, dimension) in dimensions {
        if let Some(dimension) = dimension {
            covered[start..start + span].fill(true);
            if span == 1 {
                tracks[start] = tracks[start].max(dimension);
                fixed[start] = true;
            }
        }
    }
    for (track, covered) in tracks.iter_mut().zip(covered) {
        if !covered {
            *track = default;
        }
    }
    let mut spans: Vec<_> = dimensions
        .iter()
        .filter(|(_, span, dimension)| *span > 1 && dimension.is_some())
        .collect();
    spans.sort_by_key(|(start, span, _)| (*span, *start));
    for &&(start, span, dimension) in &spans {
        let minimum = u64::from(dimension.unwrap());
        let current: u64 = tracks[start..start + span]
            .iter()
            .map(|&h| u64::from(h))
            .sum();
        if current >= minimum {
            continue;
        }
        let mut flexible: Vec<_> = (start..start + span).filter(|&i| !fixed[i]).collect();
        if flexible.is_empty() {
            flexible.extend(start..start + span);
        }
        let remaining = minimum - current;
        let count = flexible.len() as u64;
        for (index, track) in flexible.into_iter().enumerate() {
            let previous = remaining * index as u64 / count;
            let next = remaining * (index as u64 + 1) / count;
            tracks[track] = tracks[track]
                .checked_add((next - previous) as u32)
                .ok_or_else(invalid)?;
        }
    }
    Ok(tracks)
}

impl DocumentCore {
    pub(crate) fn parse_table_html(
        &mut self,
        paragraphs: &mut Vec<Paragraph>,
        table_html: &str,
        width: u32,
    ) -> Result<(), crate::error::HwpError> {
        use crate::model::control::Control;
        use crate::model::table::{Cell, Table, TablePageBreak};

        // --- 1. HTML 파싱: 행/셀 구조 추출 ---
        let table_lower = table_html.to_lowercase();

        struct ParsedCell {
            col_span: u16,
            row_span: u16,
            width_pt: f64,
            height_pt: f64,
            padding_pt: [f64; 4],          // left, right, top, bottom
            border_widths_pt: [f64; 4],    // left, right, top, bottom
            border_colors: [u32; 4],       // BGR
            border_styles: [u8; 4],        // 0=none, 1=solid, 2=dashed, 3=dotted, 4=double
            background_color: Option<u32>, // BGR
            content_html: String,
            is_header: bool,
            vertical_align: u8, // 0=top, 1=center, 2=bottom
        }

        let mut parsed_rows: Vec<Vec<ParsedCell>> = Vec::new();

        let mut pos = 0;
        while let Some(tr_start) = table_lower[pos..].find("<tr") {
            let tr_abs = pos + tr_start;
            let tr_end = find_closing_tag(table_html, tr_abs, "tr");
            let tr_inner = &table_html[tr_abs..tr_end.min(table_html.len())];
            let tr_inner_lower = tr_inner.to_lowercase();

            let mut row_cells: Vec<ParsedCell> = Vec::new();

            // <td>와 <th>를 출현 순서대로 처리
            let mut td_pos = 0;
            loop {
                let td_match = tr_inner_lower[td_pos..].find("<td");
                let th_match = tr_inner_lower[td_pos..].find("<th");

                let (tag_offset, is_th) = match (td_match, th_match) {
                    (Some(a), Some(b)) => {
                        if a <= b {
                            (a, false)
                        } else {
                            (b, true)
                        }
                    }
                    (Some(a), None) => (a, false),
                    (None, Some(b)) => (b, true),
                    (None, None) => break,
                };

                let cell_abs = td_pos + tag_offset;
                let tag_name = if is_th { "th" } else { "td" };

                // <td ...> 태그에서 속성 추출
                if let Some(gt) = tr_inner[cell_abs..].find('>') {
                    let tag_str = &tr_inner[cell_abs..cell_abs + gt + 1];

                    // colspan / rowspan 파싱
                    let col_span = parse_html_attr_u16(tag_str, "colspan").unwrap_or(1).max(1);
                    let row_span = parse_html_attr_u16(tag_str, "rowspan").unwrap_or(1).max(1);

                    // 인라인 style 파싱
                    let css = parse_inline_style(tag_str);
                    let css_lower = css.to_lowercase();

                    // 크기 파싱
                    let width_pt = parse_css_dimension_pt(&css_lower, "width");
                    let height_pt = parse_css_dimension_pt(&css_lower, "height");

                    // 패딩 파싱
                    let padding_pt = parse_css_padding_pt(&css_lower);

                    // 테두리 파싱 (left, right, top, bottom)
                    let mut border_widths_pt = [0.0f64; 4];
                    let mut border_colors = [0u32; 4]; // black
                    let mut border_styles = [1u8; 4]; // solid default

                    // 축약형 border 먼저
                    if let Some(bval) = parse_css_value(&css_lower, "border") {
                        let (w, c, s) = parse_css_border_shorthand(&bval);
                        for i in 0..4 {
                            border_widths_pt[i] = w;
                            border_colors[i] = c;
                            border_styles[i] = s;
                        }
                    }
                    // 개별 방향 오버라이드
                    let sides = ["border-left", "border-right", "border-top", "border-bottom"];
                    for (i, side) in sides.iter().enumerate() {
                        if let Some(bval) = parse_css_value(&css_lower, side) {
                            let (w, c, s) = parse_css_border_shorthand(&bval);
                            border_widths_pt[i] = w;
                            border_colors[i] = c;
                            border_styles[i] = s;
                        }
                    }

                    // 배경색
                    let background_color = parse_css_value(&css_lower, "background-color")
                        .or_else(|| parse_css_value(&css_lower, "background"))
                        .and_then(|v| css_color_to_hwp_bgr(&v));

                    // 수직 정렬 (0=미지정, 1=center, 2=bottom, 3=명시적 top)
                    let vertical_align =
                        match parse_css_value(&css_lower, "vertical-align").as_deref() {
                            Some("middle") | Some("center") => 1u8,
                            Some("bottom") => 2u8,
                            Some("top") => 3u8, // 명시적 top
                            _ => 0u8,           // 미지정 → Center (HWP 기본)
                        };

                    // 셀 내용 HTML 추출 — 셀 안에 같은 태그 이름의 중첩 표(예:
                    // 우리 clipboard export 가 내보내는 중첩 <table>도 셀에
                    // <td>를 쓴다)가 있으면 얕은 find 는 안쪽 셀의 닫는 태그에서
                    // 먼저 멈춰 바깥 셀 내용을 잘라먹는다(#4413 왕복 검증 중
                    // 발견). <tr> 경계 탐색에 이미 쓰는 깊이 추적 헬퍼를 그대로
                    // 재사용해 같은 태그 이름의 중첩을 건너뛴다.
                    let content_start = cell_abs + gt + 1;
                    let close_tag = format!("</{}>", tag_name);
                    let cell_close = find_closing_tag(tr_inner, cell_abs, tag_name);
                    let content_end = cell_close
                        .saturating_sub(close_tag.len())
                        .max(content_start)
                        .min(tr_inner.len());
                    let content_html = tr_inner[content_start..content_end].to_string();

                    row_cells.push(ParsedCell {
                        col_span,
                        row_span,
                        width_pt,
                        height_pt,
                        padding_pt,
                        border_widths_pt,
                        border_colors,
                        border_styles,
                        background_color,
                        content_html,
                        is_header: is_th,
                        vertical_align,
                    });

                    td_pos = content_end
                        .saturating_add(close_tag.len())
                        .min(tr_inner_lower.len());
                } else {
                    break;
                }
            }

            // rowspan이 덮은 빈 행도 뒤 행의 주소를 차지한다.
            parsed_rows.push(row_cells);
            pos = tr_end;
        }

        if parsed_rows.iter().all(Vec::is_empty) {
            return Ok(());
        }

        // --- 2. 그리드 정규화: 실제 col 인덱스 계산 ---
        let invalid_grid =
            || crate::error::HwpError::RenderError("표의 행이나 열이 너무 많습니다".to_string());
        let mut row_count = u16::try_from(parsed_rows.len()).map_err(|_| invalid_grid())?;
        // 행×열 배열 대신 열마다 마지막 점유 행만 보관한다.
        let mut occupied_until: Vec<usize> = Vec::new();

        struct CellPos {
            row: u16,
            col: u16,
            col_span: u16,
            row_span: u16,
            parsed_row: usize,
            parsed_col: usize,
        }

        let mut cell_positions: Vec<CellPos> = Vec::new();
        let mut actual_col_count: u16 = 0;

        for (ri, row) in parsed_rows.iter().enumerate() {
            let mut col_cursor: usize = 0;
            for (ci, cell) in row.iter().enumerate() {
                let end_row = ri + usize::from(cell.row_span);
                row_count = row_count.max(u16::try_from(end_row).map_err(|_| invalid_grid())?);
                let end_col = loop {
                    let end = col_cursor + usize::from(cell.col_span);
                    u16::try_from(end).map_err(|_| invalid_grid())?;
                    if let Some(occupied) = (col_cursor..end)
                        .find(|&col| occupied_until.get(col).is_some_and(|&until| until > ri))
                    {
                        col_cursor = occupied + 1;
                    } else {
                        break end;
                    }
                };
                let col = col_cursor as u16;
                occupied_until.resize(occupied_until.len().max(end_col), 0);
                occupied_until[col_cursor..end_col].fill(end_row);

                cell_positions.push(CellPos {
                    row: ri as u16,
                    col,
                    col_span: cell.col_span,
                    row_span: cell.row_span,
                    parsed_row: ri,
                    parsed_col: ci,
                });

                actual_col_count = actual_col_count.max(end_col as u16);
                col_cursor = end_col;
            }
        }

        let col_count = actual_col_count.max(1);
        if usize::from(row_count) * usize::from(col_count)
            > crate::model::table::MAX_TABLE_GRID_CELLS
        {
            return Err(invalid_grid());
        }

        // 표 문단의 para_shape_id: 기존 문서의 표 문단에서 사용하는 값 탐색
        // 정상 파일에서 표 문단은 ps_id=1 사용 (기본 "본문" 스타일)
        let table_para_shape_id = {
            let mut found_ps = 0u16;
            'outer: for section in &self.document.sections {
                for para in &section.paragraphs {
                    for ctrl in &para.controls {
                        if let Control::Table(_) = ctrl {
                            found_ps = para.para_shape_id;
                            break 'outer;
                        }
                    }
                }
            }
            if found_ps == 0 && self.document.doc_info.para_shapes.len() > 1 {
                1u16 // 기본 "본문" ParaShape
            } else {
                found_ps
            }
        };

        // --- 3. 셀 크기 계산 ---
        let outer_margin: i16 = 283;
        let paragraph_margin = self
            .document
            .doc_info
            .para_shapes
            .get(table_para_shape_id as usize)
            .map(|shape| {
                (shape.margin_left as i64 + shape.margin_right as i64 + shape.indent.max(0) as i64)
                    / 2
            })
            .unwrap_or(0);
        let available_width = width as i64 - paragraph_margin - i64::from(outer_margin) * 2;
        if available_width < i64::from(col_count) {
            return Err(crate::error::HwpError::RenderError(
                "표를 넣을 가로 공간이 부족합니다. 문단이나 셀의 여백을 줄여 주세요".to_string(),
            ));
        }
        let available_width = available_width as u32;
        let default_col_width = available_width / col_count as u32;
        let default_row_height: u32 = 1000;

        let dimension = |pt: f64| {
            let hu = (pt * 100.0).round();
            (pt.is_finite() && pt > 0.0 && hu.is_finite() && hu <= f64::from(u32::MAX))
                .then_some((hu as u32).max(1))
        };
        // 병합 칸도 미지정 열의 기준 폭을 제공한다. 전체 병합 표에는 한 열짜리 셀이 없다.
        let widths: Vec<_> = cell_positions
            .iter()
            .map(|cp| {
                let pc = &parsed_rows[cp.parsed_row][cp.parsed_col];
                (
                    usize::from(cp.col),
                    usize::from(cp.col_span),
                    dimension(pc.width_pt),
                )
            })
            .collect();
        let base_col_widths =
            html_table_tracks(usize::from(col_count), &widths, default_col_width)?;
        let mut col_widths = base_col_widths.clone();

        let tag = &table_html[..table_html.find('>').map_or(0, |end| end + 1)];
        let css = parse_inline_style(tag).to_lowercase();
        // 일반 CSS는 최소 치수다. 원문 기하를 명시한 복사 조각만 셀별 저장 치수를 그대로 읽는다.
        let exact_geometry =
            extract_html_attr(tag, "data-rhwp-geometry").as_deref() == Some("exact");
        let requested_width = parse_css_value(&css, "width")
            .and_then(|value| {
                value
                    .strip_suffix('%')
                    .and_then(|n| n.trim().parse::<f64>().ok())
            })
            .map(|percent| available_width as f64 * percent / 100.0)
            .unwrap_or_else(|| parse_css_dimension_pt(&css, "width") * 100.0);
        let total: u64 = col_widths.iter().map(|&width| u64::from(width)).sum();
        if total == 0 {
            return Err(crate::error::HwpError::RenderError(
                "표의 너비가 올바르지 않습니다".to_string(),
            ));
        }
        let declared_width = dimension(parse_css_dimension_pt(&css, "width"));
        let source_width = declared_width.map_or(total, u64::from);
        // 클립보드의 원문 표시만으로 치수를 신뢰하지 않는다. rowspan 칸을 포함한 각 행이 선언 폭 안에 있어야 한다.
        let exact_width = if exact_geometry {
            let mut row_widths = vec![0u64; usize::from(row_count)];
            for (cp, &(_, _, own_width)) in cell_positions.iter().zip(&widths) {
                let cell_width = own_width.map_or_else(
                    || {
                        base_col_widths[usize::from(cp.col)..usize::from(cp.col + cp.col_span)]
                            .iter()
                            .map(|&width| u64::from(width))
                            .sum()
                    },
                    u64::from,
                );
                for row in cp.row..cp.row + cp.row_span {
                    row_widths[usize::from(row)] += cell_width;
                }
            }
            declared_width.is_some_and(|width| {
                row_widths
                    .into_iter()
                    .all(|row_width| row_width <= u64::from(width))
            })
        } else {
            false
        };
        let target = if requested_width.is_finite() && requested_width > 0.0 {
            (requested_width.round() as u32).min(available_width)
        } else {
            total.min(u64::from(available_width)) as u32
        }
        .max(u32::from(col_count));
        // 최종 폭으로 셀·저장 바이트·줄 폭을 함께 만들어 저장 후에도 같은 너비를 쓴다.
        if total != u64::from(target) {
            let mut consumed = 0;
            let mut assigned = 0;
            for width in &mut col_widths {
                consumed += u64::from(*width);
                let end = (u128::from(consumed) * u128::from(target - u32::from(col_count))
                    / u128::from(total)) as u64;
                *width = (end - assigned) as u32 + 1;
                assigned = end;
            }
        }

        let heights: Vec<_> = cell_positions
            .iter()
            .map(|cp| {
                let pc = &parsed_rows[cp.parsed_row][cp.parsed_col];
                (
                    usize::from(cp.row),
                    usize::from(cp.row_span),
                    dimension(pc.height_pt),
                )
            })
            .collect();
        let row_heights = html_table_tracks(usize::from(row_count), &heights, default_row_height)?;
        let computed_height: u64 = row_heights.iter().map(|&height| u64::from(height)).sum();
        // 표 전체 높이에는 셀 합보다 큰 원래 여유가 있을 수 있다. 셀 치수는 늘리지 않는다.
        let declared_height = dimension(parse_css_dimension_pt(&css, "height"));
        let total_height = match (exact_geometry, declared_height) {
            (true, Some(height)) => height,
            _ => u32::try_from(computed_height)
                .map_err(|_| {
                    crate::error::HwpError::RenderError("표의 높이가 너무 큽니다".to_string())
                })?
                .max(declared_height.unwrap_or(0)),
        };

        // HTML <table> CSS에서 표 패딩 파싱
        let table_style =
            parse_inline_style(&table_html[..table_html.find('>').map_or(0, |end| end + 1)])
                .to_lowercase();
        let table_padding_pt = parse_css_padding_pt(&table_style);
        // 기본값: L:510 R:510 T:141 B:141 (정상 HWP 파일 패턴)
        let table_padding = crate::model::Padding {
            left: if table_padding_pt[0] > 0.01 {
                (table_padding_pt[0] * 100.0).round() as i16
            } else {
                510
            },
            right: if table_padding_pt[1] > 0.01 {
                (table_padding_pt[1] * 100.0).round() as i16
            } else {
                510
            },
            top: if table_padding_pt[2] > 0.01 {
                (table_padding_pt[2] * 100.0).round() as i16
            } else {
                141
            },
            bottom: if table_padding_pt[3] > 0.01 {
                (table_padding_pt[3] * 100.0).round() as i16
            } else {
                141
            },
        };

        // --- 4. BorderFill 생성 및 Cell 구조체 조립 ---
        let mut cells: Vec<Cell> = Vec::new();
        let mut has_header_row = false;

        for cp in &cell_positions {
            let pc = &parsed_rows[cp.parsed_row][cp.parsed_col];

            // 셀 폭/높이 (병합 고려)
            let mut cell_width: u32 = (cp.col..cp.col + cp.col_span)
                .map(|c| {
                    col_widths
                        .get(c as usize)
                        .copied()
                        .unwrap_or(default_col_width)
                })
                .sum();
            let base_width: u64 = base_col_widths
                [usize::from(cp.col)..usize::from(cp.col + cp.col_span)]
                .iter()
                .map(|&width| u64::from(width))
                .sum();
            if exact_width {
                if let Some(own_width) = dimension(pc.width_pt) {
                    if source_width <= u64::from(available_width) {
                        cell_width = own_width;
                    } else if u64::from(own_width) != base_width {
                        // 공통 열에 맞는 칸은 기존 폭 제한을 쓰고, 행별 다른 칸만 원문 비율로 줄인다.
                        cell_width = ((u128::from(own_width) * u128::from(target)
                            + u128::from(source_width) / 2)
                            / u128::from(source_width))
                        .clamp(1, u128::from(target)) as u32;
                    }
                }
            }
            let row_height: u64 = (cp.row..cp.row + cp.row_span)
                .map(|r| {
                    row_heights
                        .get(r as usize)
                        .copied()
                        .map(u64::from)
                        .unwrap_or(u64::from(default_row_height))
                })
                .sum();
            let cell_height = match dimension(pc.height_pt).filter(|_| exact_geometry) {
                Some(height) => height,
                None => u32::try_from(row_height).map_err(|_| {
                    crate::error::HwpError::RenderError("셀의 높이가 너무 큽니다".to_string())
                })?,
            };

            // BorderFill 생성/재사용
            let border_fill_id = self.create_border_fill_from_css(
                &pc.border_widths_pt,
                &pc.border_colors,
                &pc.border_styles,
                pc.background_color,
            );

            // 패딩 (pt → HWPUNIT16, CSS 미지정 시 기본 1.4mm ≈ 397 HWPUNIT)
            let default_pad: f64 = 141.0; // ~0.5mm HWPUNIT
            let padding = crate::model::Padding {
                left: if pc.padding_pt[0] > 0.01 {
                    (pc.padding_pt[0] * 100.0).round() as i16
                } else {
                    default_pad as i16
                },
                right: if pc.padding_pt[1] > 0.01 {
                    (pc.padding_pt[1] * 100.0).round() as i16
                } else {
                    default_pad as i16
                },
                top: if pc.padding_pt[2] > 0.01 {
                    (pc.padding_pt[2] * 100.0).round() as i16
                } else {
                    default_pad as i16
                },
                bottom: if pc.padding_pt[3] > 0.01 {
                    (pc.padding_pt[3] * 100.0).round() as i16
                } else {
                    default_pad as i16
                },
            };

            // 태그 밖 출력 공백만 있으면 빈 셀이다. 실제 문단 안 공백과
            // 그림·중첩 표는 평문 추출로 판단하지 않고 파싱 결과를 보존한다.
            let cell_paragraphs = if pc.content_html.trim().is_empty() {
                vec![Paragraph::new_empty()]
            } else {
                let parsed = self.parse_html_to_paragraphs(
                    &pc.content_html,
                    cell_width.saturating_sub(
                        table_padding.left.max(0) as u32 + table_padding.right.max(0) as u32,
                    ),
                )?;
                if parsed.is_empty()
                    || parsed
                        .iter()
                        .all(|p| p.text.is_empty() && p.controls.is_empty())
                {
                    vec![Paragraph::new_empty()]
                } else {
                    parsed
                }
            };

            // 셀 문단 보정: char_count_msb, char_count, raw_header_extra, line_segs
            let mut cell_paragraphs = cell_paragraphs;
            for cp_para in &mut cell_paragraphs {
                cp_para.char_count_msb = true; // 셀 문단은 항상 MSB 설정
                                               // char_count에 문단끝 마커(+1) 포함
                let text_chars = cp_para.text.chars().count() as u32;
                // 컨트롤(그림·중첩 표)은 확장 제어문자 8 코드유닛을 차지한다.
                // 그 자리를 빼먹으면 저장 때 컨트롤이 통째로 사라진다(셀 안 그림 실측).
                let control_units = cp_para.controls.len() as u32 * 8;
                cp_para.char_count = text_chars + control_units + 1;

                // [#4275] CSS 에서 만든 para_shape_id 를 보존한다. 종전에는 무조건 0 으로
                // 덮어 정렬·줄간격이 교차 문서 HTML 붙여넣기에서 사라졌다.

                // DIFF-2: char_shapes가 비어있으면 기본 CharShapeRef 추가
                // 모든 셀 문단은 최소 1개의 명시적 CharShapeRef를 가져야 함
                if cp_para.char_shapes.is_empty() {
                    let base_cs_id = if !self.document.doc_info.char_shapes.is_empty() {
                        0u32
                    } else {
                        0u32
                    };
                    cp_para
                        .char_shapes
                        .push(crate::model::paragraph::CharShapeRef {
                            start_pos: 0,
                            char_shape_id: base_cs_id,
                        });
                }

                // raw_header_extra에 instance_id = 0x80000000 설정
                if cp_para.raw_header_extra.len() >= 10 {
                    cp_para.raw_header_extra[6..10].copy_from_slice(&0x80000000u32.to_le_bytes());
                } else {
                    let mut rhe = vec![0u8; 10];
                    let n_cs = cp_para.char_shapes.len() as u16;
                    rhe[0..2].copy_from_slice(&n_cs.to_le_bytes());
                    // [2..4] n_range_tags = 0
                    let n_ls = cp_para.line_segs.len().max(1) as u16;
                    rhe[4..6].copy_from_slice(&n_ls.to_le_bytes());
                    rhe[6..10].copy_from_slice(&0x80000000u32.to_le_bytes());
                    cp_para.raw_header_extra = rhe;
                }

                // line_segs: 폰트 크기 기반 높이 계산
                let font_size = cp_para
                    .char_shapes
                    .first()
                    .and_then(|cs| {
                        self.document
                            .doc_info
                            .char_shapes
                            .get(cs.char_shape_id as usize)
                    })
                    .map(|cs| cs.base_size.max(400))
                    .unwrap_or(1000);
                let line_h = font_size;
                let text_h = font_size;
                let baseline = (font_size as f64 * 0.85) as i32;
                let spacing = (font_size as f64 * 0.6) as i32;
                // seg_width: 셀 폭에서 좌우 패딩을 뺀 텍스트 영역 폭
                let seg_w = cell_width.saturating_sub(
                    table_padding.left.max(0) as u32 + table_padding.right.max(0) as u32,
                ) as i32;
                // tag(flags): bit 17(first segment) + bit 18(last segment).
                let line_tag: u32 = crate::model::paragraph::LineSeg::TAG_SINGLE_SEGMENT_LINE;

                if cp_para.line_segs.is_empty() {
                    cp_para.line_segs.push(crate::model::paragraph::LineSeg {
                        text_start: 0,
                        line_height: line_h,
                        text_height: text_h,
                        baseline_distance: baseline,
                        line_spacing: spacing,
                        segment_width: seg_w,
                        tag: line_tag,
                        ..Default::default()
                    });
                } else {
                    for ls in &mut cp_para.line_segs {
                        if ls.line_height < font_size {
                            ls.line_height = line_h;
                            ls.text_height = text_h;
                            ls.baseline_distance = baseline;
                            ls.line_spacing = spacing;
                        }
                        if ls.segment_width == 0 {
                            ls.segment_width = seg_w;
                        }
                        if ls.tag == 0 {
                            ls.tag = line_tag;
                        }
                    }
                }
            }

            if pc.is_header {
                has_header_row = true;
            }

            // list_header_width_ref: is_header면 bit 2 설정
            let lh_width_ref: u16 = if pc.is_header { 0x04 } else { 0 };

            // vertical_align → VerticalAlign enum
            // CSS에서 지정하지 않으면 기본값 Center (정상 HWP 파일 패턴)
            let v_align = match pc.vertical_align {
                0 => crate::model::table::VerticalAlign::Center, // CSS 미지정 → Center (HWP 기본)
                1 => crate::model::table::VerticalAlign::Center,
                2 => crate::model::table::VerticalAlign::Bottom,
                3 => crate::model::table::VerticalAlign::Top, // 명시적 top
                _ => crate::model::table::VerticalAlign::Center,
            };

            // raw_list_extra: 13바이트 (첫 4바이트 = 셀 폭 u32, 나머지 0)
            // 정상 파일에서 raw_list_extra[0..4] = cell_width (u32)
            let mut raw_list_extra = vec![0u8; 13];
            raw_list_extra[0..4].copy_from_slice(&cell_width.to_le_bytes());

            cells.push(Cell {
                col: cp.col,
                row: cp.row,
                col_span: cp.col_span,
                row_span: cp.row_span,
                width: cell_width,
                height: cell_height,
                padding,
                border_fill_id,
                paragraphs: cell_paragraphs,
                is_header: pc.is_header,
                list_header_width_ref: lh_width_ref,
                vertical_align: v_align,
                raw_list_extra,
                ..Default::default()
            });
        }

        // 행 우선 순서로 정렬
        cells.sort_by(|a, b| a.row.cmp(&b.row).then(a.col.cmp(&b.col)));

        // --- 5. Table 구조체 조립 ---
        let total_width: u32 = col_widths.iter().sum();

        // table.attr: 기존 문서의 표와 동일한 패턴 사용
        // 0x082A2311 = treat_as_char | vert_rel_to=Para | horz_rel_to=Column |
        //              allow_overlap | width_criterion | various layout flags
        // 정상 HWP 파일의 모든 표에서 사용되는 표준값
        let table_attr: u32 = 0x082A2311;

        // raw_ctrl_data: CommonObjAttr 전체 (attr 포함, parse_common_obj_attr 정합)
        // [0..4] attr, [4..8] vertical_offset, [8..12] horizontal_offset,
        // [12..16] width, [16..20] height, [20..24] z_order,
        // [24..26] margin.left, [26..28] margin.right,
        // [28..30] margin.top, [30..32] margin.bottom,
        // [32..36] instance_id, [36..38] desc_len(=0)
        let mut raw_ctrl_data = vec![0u8; 38]; // 32(base) + 2(desc_len) + 4(extra)
        raw_ctrl_data[common_obj_offsets::FLAGS].copy_from_slice(&table_attr.to_le_bytes());
        raw_ctrl_data[common_obj_offsets::WIDTH].copy_from_slice(&total_width.to_le_bytes());
        raw_ctrl_data[common_obj_offsets::HEIGHT].copy_from_slice(&total_height.to_le_bytes());
        // 바깥 여백 (left, right, top, bottom)
        raw_ctrl_data[common_obj_offsets::MARGIN_LEFT].copy_from_slice(&outer_margin.to_le_bytes());
        raw_ctrl_data[common_obj_offsets::MARGIN_RIGHT]
            .copy_from_slice(&outer_margin.to_le_bytes());
        raw_ctrl_data[common_obj_offsets::MARGIN_TOP].copy_from_slice(&outer_margin.to_le_bytes());
        raw_ctrl_data[common_obj_offsets::MARGIN_BOTTOM]
            .copy_from_slice(&outer_margin.to_le_bytes());
        // [32..36] instance_id — 공용 할당기로 받은 고유 개체 id.
        //
        // [#7231] 종전에는 행·열 수와 셀 수·전체 폭·높이로 만든 해시를 썼다. 크기가 같은
        // 표끼리 같은 값이 나오고, `common.instance_id` 는 0 으로 남아 HWPX `<hp:tbl id>`
        // 가 `"0"` 이 됐다. `model/identity.rs` 의 주석이 그 함정을 적어 뒀다 —
        // *"dimensions, clock time and wrapping hashes cannot establish uniqueness"*.
        //
        // 이 함수는 HTML 하나에 표가 여러 개면 **반복 호출**되고, 만든 표는 아직
        // `self.document` 에 들어가지 않은 `paragraphs` 에 쌓인다. 그래서 문서의 사용
        // 집합에 이번 붙여넣기 분(`paragraph_ids`)까지 더해 예약한다.
        let instance_id: u32 = {
            let mut used = crate::model::identity::used_instance_ids(&self.document);
            used.extend(crate::model::identity::paragraph_ids(paragraphs));
            crate::model::identity::Allocator { used, next: 1 }
                .id()
                .unwrap_or(0)
        };
        raw_ctrl_data[common_obj_offsets::INSTANCE_ID].copy_from_slice(&instance_id.to_le_bytes());
        // [36..38] desc_len = 0

        // row_sizes: 각 행의 셀 수
        let row_sizes: Vec<i16> = (0..row_count)
            .map(|r| cells.iter().filter(|c| c.row == r).count() as i16)
            .collect();

        // 표 전체 기본 BorderFill: 정상 파일에서 모든 표가 border_fill_id=3 사용
        // border_fill_id는 1-based (DocInfo.border_fills 인덱스 + 1)
        let table_border_fill_id = if self.document.doc_info.border_fills.len() >= 3 {
            3u16
        } else if !self.document.doc_info.border_fills.is_empty() {
            1u16
        } else {
            0u16
        };

        // raw_table_record_attr: 정상 파일 패턴 기반 (DIFF-5 수정)
        // bit 1: 셀 분리 금지 (항상 설정), bit 2: repeat_header
        // bit 26: 추가 레이아웃 속성
        // 정상 HWP 파일에서 모든 표는 bit 1 (셀 분리 금지) 이 항상 설정됨
        let tbl_rec_attr: u32 = 0x04000006; // bit 1(셀분리금지) + bit 2 + bit 26

        let mut table = Table {
            attr: table_attr,
            row_count,
            col_count,
            cell_spacing: 0,
            padding: table_padding,
            row_sizes,
            border_fill_id: table_border_fill_id,
            zones: Vec::new(),
            cells,
            cell_grid: Vec::new(),
            page_break: TablePageBreak::None,
            repeat_header: has_header_row,
            caption: None,
            // raw attr(0x082A2311)엔 '글자처럼 취급·문단 기준'이 들어 있지만 렌더러·HWPX
            // 내보내기는 파싱된 `common` 을 읽는다. Default(treat_as_char=false, PAPER 기준 0,0)로 두면
            // 붙여넣은 표가 전부 종이 좌상단에 겹쳐 놓인다(실측). 편집기 표 삽입(object_ops/table.rs)과 동일값.
            common: crate::model::shape::CommonObjAttr {
                treat_as_char: true,
                text_wrap: crate::model::shape::TextWrap::TopAndBottom,
                vert_rel_to: crate::model::shape::VertRelTo::Page,
                horz_rel_to: crate::model::shape::HorzRelTo::Para,
                vert_align: crate::model::shape::VertAlign::Top,
                horz_align: crate::model::shape::HorzAlign::Left,
                width: total_width,
                height: total_height,
                // [#7231] IR 과 raw 가 같은 값을 갖는다 — HWPX 저장기는 이 필드를,
                // HWP5 쪽은 `raw_ctrl_data` 를 읽는다.
                instance_id,
                ..Default::default()
            },
            outer_margin_left: outer_margin,
            outer_margin_right: outer_margin,
            outer_margin_top: outer_margin,
            outer_margin_bottom: outer_margin,
            raw_ctrl_data,
            raw_ctrl_seal: None,
            raw_table_record_attr: tbl_rec_attr,
            raw_table_record_extra: vec![0u8; 2], // 표준 추가 2바이트
        };
        table.rebuild_grid();

        // --- 6. Table Control을 포함하는 Paragraph 생성 ---
        // 제어문자는 text에 포함하지 않음 (serialize_para_text가 controls에서 생성)
        let default_char_shape_id = if !self.document.doc_info.char_shapes.is_empty() {
            0u32
        } else {
            self.document
                .doc_info
                .char_shapes
                .push(crate::model::style::CharShape::default());
            0
        };

        // raw_header_extra: [0..2] n_char_shapes, [2..4] n_range_tags, [4..6] n_line_segs, [6..10] instance_id
        // 정상 파일에서 표 문단의 instance_id = 0x80000000
        let mut table_raw_header_extra = vec![0u8; 10];
        table_raw_header_extra[0..2].copy_from_slice(&1u16.to_le_bytes()); // n_char_shapes=1
                                                                           // [2..4] n_range_tags=0, [4..6] n_line_segs=1
        table_raw_header_extra[4..6].copy_from_slice(&1u16.to_le_bytes());
        // [6..10] instance_id=0x80000000
        table_raw_header_extra[6..10].copy_from_slice(&0x80000000u32.to_le_bytes());

        let table_para = Paragraph {
            text: String::new(),
            char_count: 9, // 확장 제어문자(8 code units) + 문단끝(1 code unit)
            control_mask: 0x00000800, // DrawTableObject (bit 11)
            char_offsets: vec![],
            char_shapes: vec![crate::model::paragraph::CharShapeRef {
                start_pos: 0,
                char_shape_id: default_char_shape_id,
            }],
            line_segs: vec![crate::model::paragraph::LineSeg {
                text_start: 0,
                line_height: total_height.min(i32::MAX as u32) as i32,
                text_height: total_height.min(i32::MAX as u32) as i32,
                baseline_distance: (total_height as f64 * 0.85).min(i32::MAX as f64) as i32,
                line_spacing: 600,
                segment_width: total_width.min(i32::MAX as u32) as i32,
                tag: crate::model::paragraph::LineSeg::TAG_SINGLE_SEGMENT_LINE,
                ..Default::default()
            }],
            para_shape_id: table_para_shape_id,
            style_id: 0,
            controls: vec![Control::Table(Box::new(table))],
            ctrl_data_records: vec![None],
            has_para_text: true,
            raw_header_extra: table_raw_header_extra,
            // 표 문단 자체의 MSB는 false (기존 HWP 문서 패턴)
            // FIX-1은 빈 문서 케이스에만 해당, 내용이 있는 문서에서는 false
            // 셀 내부 문단의 MSB는 true (셀 보정 코드에서 설정)
            char_count_msb: false,
            ..Default::default()
        };

        paragraphs.push(table_para);
        Ok(())
    }

    /// CSS 테두리/배경 정보로 BorderFill을 생성하고 DocInfo에 등록한다.
    /// 동일한 BorderFill이 이미 있으면 기존 ID를 반환한다.
    pub(crate) fn create_border_fill_from_css(
        &mut self,
        border_widths_pt: &[f64; 4],
        border_colors: &[u32; 4],
        border_styles: &[u8; 4],
        background_color: Option<u32>,
    ) -> u16 {
        use crate::model::style::{
            BorderFill, BorderLine, BorderLineType, CenterLine, DiagonalLine, Fill, FillType,
            SolidFill,
        };

        let mut borders = [BorderLine::default(); 4];
        for i in 0..4 {
            if border_widths_pt[i] > 0.01 {
                borders[i] = BorderLine {
                    line_type: match border_styles[i] {
                        0 => BorderLineType::None,
                        1 => BorderLineType::Solid,
                        2 => BorderLineType::Dash,
                        3 => BorderLineType::Dot,
                        4 => BorderLineType::Double,
                        _ => BorderLineType::Solid,
                    },
                    width: css_border_width_to_hwp(border_widths_pt[i]),
                    color: border_colors[i],
                };
            } else {
                borders[i] = BorderLine {
                    line_type: BorderLineType::None,
                    width: 0,
                    color: 0,
                };
            }
        }

        let fill = if let Some(bg) = background_color {
            if bg != 0xFFFFFF {
                Fill {
                    fill_type: FillType::Solid,
                    solid: Some(SolidFill {
                        background_color: bg,
                        pattern_color: 0,
                        pattern_type: -1_i32, // 무늬 없음
                    }),
                    ..Default::default()
                }
            } else {
                Fill::default()
            }
        } else {
            Fill::default()
        };

        let bf = BorderFill {
            raw_data: None,
            attr: 0,
            borders,
            diagonal: DiagonalLine::default(),
            center_line: CenterLine::None,
            fill,
            three_d: false,
        };

        // 기존 BorderFill에서 동일한 항목 검색
        for (i, existing) in self.document.doc_info.border_fills.iter().enumerate() {
            if border_fills_equal(existing, &bf) {
                return (i + 1) as u16; // border_fill_id는 1-based
            }
        }

        // 새로 추가
        self.document.doc_info.border_fills.push(bf);
        self.document.doc_info.raw_stream_dirty = true;
        self.rebuild_resolved_styles();
        self.document.doc_info.border_fills.len() as u16
    }

    /// JSON에서 border/fill 속성을 파싱하여 BorderFill을 생성/재사용한다.
    /// 프론트엔드 글자 테두리/배경 대화상자에서 호출된다.
    pub(crate) fn create_border_fill_from_json(&mut self, json: &str) -> u16 {
        use crate::model::style::{
            BorderFill, BorderLine, CenterLine, DiagonalLine, Fill, FillType, SolidFill,
        };

        fn json_diag_bits(json: &str, key: &str) -> Option<u16> {
            json_i32(json, key)
                .map(|v| (v as u16) & 0x07)
                .or_else(|| json_bool(json, key).map(|v| if v { 0b010 } else { 0 }))
        }

        fn json_center_line(json: &str) -> Option<CenterLine> {
            json_str(json, "centerLine").map(|value| {
                let normalized = value.trim().to_ascii_uppercase();
                match normalized.as_str() {
                    "VERTICAL" | "HORIZONTAL_BAR" => CenterLine::Vertical,
                    "HORIZONTAL" | "VERTICAL_BAR" => CenterLine::Horizontal,
                    "CROSS" => CenterLine::Cross,
                    _ => CenterLine::None,
                }
            })
        }

        let mut bf = json_u32(json, "borderFillId")
            .and_then(|id| {
                if id == 0 {
                    None
                } else {
                    self.document
                        .doc_info
                        .border_fills
                        .get((id - 1) as usize)
                        .cloned()
                }
            })
            .unwrap_or(BorderFill {
                raw_data: None,
                attr: 0,
                borders: [BorderLine::default(); 4],
                diagonal: DiagonalLine::default(),
                center_line: CenterLine::None,
                fill: Fill::default(),
                three_d: false,
            });
        bf.raw_data = None;

        // 4방향 테두리 파싱
        let dir_keys = ["borderLeft", "borderRight", "borderTop", "borderBottom"];
        for (i, key) in dir_keys.iter().enumerate() {
            if let Some(obj_str) = json_object(json, key) {
                let type_val = json_i32(&obj_str, "type").unwrap_or(0);
                bf.borders[i].line_type = u8_to_border_line_type(type_val as u8);
                bf.borders[i].width = json_i32(&obj_str, "width").unwrap_or(0) as u8;
                bf.borders[i].color = json_color(&obj_str, "color").unwrap_or(0);
            }
        }

        // 채우기 파싱
        if let Some(fill_type_str) = json_str(json, "fillType") {
            bf.fill = if fill_type_str == "solid" {
                let bg = json_color(json, "fillColor").unwrap_or(0xFFFFFF);
                let pat_c = json_color(json, "patternColor").unwrap_or(0);
                let pat_t = json_i32(json, "patternType").unwrap_or(0);
                Fill {
                    fill_type: FillType::Solid,
                    solid: Some(SolidFill {
                        background_color: bg,
                        pattern_color: pat_c,
                        pattern_type: pat_t,
                    }),
                    ..Default::default()
                }
            } else {
                Fill::default()
            };
        }

        let has_diagonal_json = json.contains("\"diagonalLine\"")
            || json.contains("\"diagonalSlash\"")
            || json.contains("\"diagonalBackSlash\"")
            || json.contains("\"diagonalWidth\"")
            || json.contains("\"diagonalColor\"")
            || json.contains("\"centerLine\"");
        if has_diagonal_json {
            let slash_bits_json = json_diag_bits(json, "diagonalSlash");
            let backslash_bits_json = json_diag_bits(json, "diagonalBackSlash");
            let center_line_json = json_center_line(json);
            let mut slash_bits = slash_bits_json.unwrap_or(((bf.attr >> 2) & 0x07) as u16);
            let mut backslash_bits = backslash_bits_json.unwrap_or(((bf.attr >> 5) & 0x07) as u16);
            let explicit_diagonal_bits = slash_bits_json.is_some() || backslash_bits_json.is_some();
            if let Some(center_line) = center_line_json {
                bf.center_line = center_line;
            }
            bf.diagonal.diagonal_type =
                json_i32(json, "diagonalLine").unwrap_or(bf.diagonal.diagonal_type as i32) as u8;
            bf.diagonal.width =
                json_i32(json, "diagonalWidth").unwrap_or(bf.diagonal.width as i32) as u8;
            bf.diagonal.color = json_color(json, "diagonalColor").unwrap_or(bf.diagonal.color);

            if bf.center_line != CenterLine::None
                && (center_line_json.is_some() || !explicit_diagonal_bits)
            {
                slash_bits = 0;
                backslash_bits = 0;
            } else if slash_bits != 0 || backslash_bits != 0 {
                bf.center_line = CenterLine::None;
            }

            bf.attr &= !((0x07 << 2)
                | (0x07 << 5)
                | (0x03 << 8)
                | (1 << 10)
                | (1 << 11)
                | (1 << 12)
                | (1 << 13));
            bf.attr |= (slash_bits & 0x07) << 2;
            bf.attr |= (backslash_bits & 0x07) << 5;
            bf.attr |= bf.center_line.hwp_attr_bits();
        }

        // 기존 BorderFill에서 동일한 항목 검색
        for (i, existing) in self.document.doc_info.border_fills.iter().enumerate() {
            if border_fills_equal(existing, &bf) {
                return (i + 1) as u16;
            }
        }

        // 새로 추가
        self.document.doc_info.border_fills.push(bf);
        self.document.doc_info.raw_stream_dirty = true;
        self.rebuild_resolved_styles();
        self.document.doc_info.border_fills.len() as u16
    }

    /// <img> 태그를 파싱하여 이미지 데이터를 문서에 추가한다.
    /// (base64 data URI만 지원)
    pub(crate) fn parse_img_html(&mut self, paragraphs: &mut Vec<Paragraph>, img_tag: &str) {
        let Some(src) = html_img_src(img_tag) else {
            return;
        };

        if !src.starts_with("data:") {
            // 외부 URL 이미지는 처리하지 않음 — 텍스트로 대체
            let mut para = Paragraph::default();
            para.text = "[이미지]".to_string();
            // [#3494] char_count 는 문단 종결자를 포함한다 (model/paragraph.rs:1042).
            para.char_count = para.text.encode_utf16().count() as u32 + 1;
            para.char_offsets = para
                .text
                .chars()
                .scan(0u32, |acc, c| {
                    let off = *acc;
                    *acc += c.len_utf16() as u32;
                    Some(off)
                })
                .collect();
            paragraphs.push(para);
            return;
        }

        let Some(pic) = self.html_data_img_picture(img_tag, src) else {
            return;
        };
        let (w_hu, h_hu) = (pic.common.width, pic.common.height);

        // 종전에는 본문 텍스트를 "[이미지]" 로 두고 Picture 컨트롤만 붙여
        // 저장·렌더 어느 쪽도 그림을 보지 못했다(HWPX 에 <hp:pic> 없음, 화면엔 글자 "[이미지]").
        // 표 문단(parse_table_html)과 같은 규약으로 만든다 —
        // 본문 없음 + 확장 제어문자 자리(char_count 9) + control_mask bit11 + 글자처럼 취급(인라인).
        let mut para = Paragraph::default();
        // 그림이 든 문단의 정렬(가운데 등)은 <img> 로 옮겨 실어 온다 —
        // 스튜디오가 문단 밖으로 올리면서 style 을 복사한다. 없으면 기본 문단서식.
        let img_style = extract_html_attr(img_tag, "style").unwrap_or_default();
        para.para_shape_id = if img_style.is_empty() {
            0
        } else {
            self.css_to_para_shape_id(&img_style)
        };
        para.text = String::new();
        para.char_count = 9; // 확장 제어문자(8 code units) + 문단끝(1)
        para.control_mask = 0x00000800;
        para.char_offsets = vec![];
        para.has_para_text = true;
        if self.document.doc_info.char_shapes.is_empty() {
            self.document
                .doc_info
                .char_shapes
                .push(crate::model::style::CharShape::default());
        }
        para.char_shapes = vec![crate::model::paragraph::CharShapeRef {
            start_pos: 0,
            char_shape_id: 0,
        }];
        para.line_segs = vec![crate::model::paragraph::LineSeg {
            text_start: 0,
            line_height: h_hu.min(i32::MAX as u32) as i32,
            text_height: h_hu.min(i32::MAX as u32) as i32,
            baseline_distance: (h_hu as f64 * 0.85).min(i32::MAX as f64) as i32,
            line_spacing: 600,
            segment_width: w_hu.min(i32::MAX as u32) as i32,
            tag: crate::model::paragraph::LineSeg::TAG_SINGLE_SEGMENT_LINE,
            ..Default::default()
        }];
        para.controls.push(Control::Picture(Box::new(pic)));
        para.ctrl_data_records = vec![None];

        paragraphs.push(para);
    }

    /// `data:` URI `<img>` 를 글자처럼 취급하는 그림 컨트롤로 만든다.
    /// 그림 데이터는 문서에 등록한다. 디코드할 수 없으면 None.
    pub(crate) fn html_data_img_picture(
        &mut self,
        img_tag: &str,
        src: &str,
    ) -> Option<crate::model::image::Picture> {
        // data:image/png;base64,XXXXX 파싱
        let after_data = src.strip_prefix("data:")?; // "image/png;base64,XXXXX"
        let base64_str = &after_data[after_data.find(',')? + 1..];

        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(base64_str)
            .ok()?;

        if decoded.is_empty() {
            return None;
        }

        // 종전에는 bin_data_content 에만 넣어 DocInfo BinData 레코드가 없었다 —
        // HWPX 로는 나가지만 HWP(5.0) 저장에서 그림이 통째로 사라졌다(클라우드 '웹 저장' 실측).
        // 그림 삽입 경로와 같은 register_embedded_bin_data 로 등록해 두 형식 모두에서 살아남게 한다.
        let extension = detect_clipboard_image_mime(&decoded)
            .split('/')
            .nth(1)
            .unwrap_or("png")
            .to_string();
        let new_bin_id = self.register_embedded_bin_data(&decoded, &extension);

        // width/height 추출
        let width = parse_html_attr_f64(img_tag, "width").unwrap_or(200.0);
        let height = parse_html_attr_f64(img_tag, "height").unwrap_or(150.0);

        // px → HWPUNIT
        let w_hu = crate::renderer::px_to_hwpunit(width, self.dpi) as u32;
        let h_hu = crate::renderer::px_to_hwpunit(height, self.dpi) as u32;

        // Picture 컨트롤 — insert_picture_native 와 같은 절대 크기 기준, 인라인(글자처럼 취급).
        let mut pic = crate::model::image::Picture::default();
        pic.image_attr.bin_data_id = new_bin_id;
        pic.common.ctrl_id = 0x67736F20; // 'gso '
        pic.common.attr = 0x01 | (4 << 15) | (2 << 18); // treat_as_char + 폭·높이 절대값
        pic.common.treat_as_char = true;
        pic.common.text_wrap = crate::model::shape::TextWrap::TopAndBottom;
        pic.common.width = w_hu;
        pic.common.height = h_hu;
        pic.common.vertical_offset = 0;
        pic.common.horizontal_offset = 0;
        pic.common.z_order = 1;
        // 한글이 내보낸 문서와 같은 배치 기준(문단 기준·글자처럼 취급·본문 흐름 따라감).
        pic.common.vert_rel_to = crate::model::shape::VertRelTo::Para;
        pic.common.horz_rel_to = crate::model::shape::HorzRelTo::Para;
        pic.common.flow_with_text = true;
        pic.shape_attr = crate::model::shape::ShapeComponentAttr {
            original_width: w_hu,
            original_height: h_hu,
            current_width: w_hu,
            current_height: h_hu,
            local_file_version: 1,
            render_sx: 1.0,
            render_sy: 1.0,
            ..Default::default()
        };
        pic.border_x = [0i32, 0, w_hu as i32, 0];
        pic.border_y = [w_hu as i32, h_hu as i32, 0, h_hu as i32];
        // crop 은 **원본 픽셀 좌표계**(px × 75)다. 종전에는 표시 크기(HWPUNIT)를
        // 넣어 큰 그림이 왼쪽 위 일부만 남고 잘렸다("그림이 반쪽" 신고). insert_picture_native 와 같은 규약.
        let (nat_w, nat_h) =
            image_pixel_size(&decoded).unwrap_or((width.max(1.0) as u32, height.max(1.0) as u32));
        pic.crop = crate::model::image::CropInfo {
            left: 0,
            top: 0,
            right: (nat_w * 75) as i32,
            bottom: (nat_h * 75) as i32,
        };
        // img_dim 은 crop 과 같은 좌표계(px × 75)다 — 픽셀값을 그대로 넣으면 HWPX 직렬화기가
        // imgClip 보다 작은 imgDim 을 보고 그림을 버린다(실측: <hp:pic> 자체가 사라졌다).
        pic.img_dim = (nat_w * 75, nat_h * 75);
        Some(pic)
    }
}

/// `<img>` 태그의 src 값(따옴표 양쪽 지원).
pub(crate) fn html_img_src(img_tag: &str) -> Option<&str> {
    ['"', '\''].into_iter().find_map(|quote| {
        let after = &img_tag[img_tag.find(&format!("src={quote}"))? + 5..];
        Some(&after[..after.find(quote)?])
    })
}

/// 태그에서 속성 문자열을 그대로 뽑는다(따옴표 양쪽 지원).
pub(crate) fn extract_html_attr(tag: &str, attr: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    for quote in ['"', '\''] {
        let needle = format!("{}={}", attr, quote);
        if let Some(start) = lower.find(&needle) {
            let after = &tag[start + needle.len()..];
            if let Some(end) = after.find(quote) {
                return Some(after[..end].to_string());
            }
        }
    }
    None
}

/// PNG·JPEG·BMP·GIF 헤더에서 원본 픽셀 크기를 읽는다.
/// 그림 `crop`·`img_dim` 은 **원본 픽셀 좌표계**라서 표시 크기로 채우면 그림이 잘린다.
pub(crate) fn image_pixel_size(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() > 24 && data[..8] == [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a] {
        let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        return Some((w, h));
    }
    if data.len() > 26 && data[0] == b'B' && data[1] == b'M' {
        let w = i32::from_le_bytes([data[18], data[19], data[20], data[21]]).unsigned_abs();
        let h = i32::from_le_bytes([data[22], data[23], data[24], data[25]]).unsigned_abs();
        return Some((w, h));
    }
    if data.len() > 10 && &data[..3] == b"GIF" {
        let w = u16::from_le_bytes([data[6], data[7]]) as u32;
        let h = u16::from_le_bytes([data[8], data[9]]) as u32;
        return Some((w, h));
    }
    if data.len() > 4 && data[0] == 0xFF && data[1] == 0xD8 {
        let mut i = 2usize;
        while i + 9 < data.len() {
            if data[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = data[i + 1];
            let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
            if (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC
            {
                let h = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
                let w = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
                return Some((w, h));
            }
            i += 2 + len;
        }
    }
    None
}

#[cfg(test)]
mod html_table_geometry_tests {
    use super::*;
    use crate::model::table::Table;

    #[derive(Debug, PartialEq)]
    struct Geometry {
        rows: u16,
        cols: u16,
        width: u32,
        height: u32,
        cells: Vec<(u16, u16, u16, u16, u32, u32, String)>,
    }

    fn blank() -> DocumentCore {
        let mut core = DocumentCore::new_empty();
        core.create_blank_document_native().expect("blank document");
        core
    }

    fn table(core: &DocumentCore) -> (usize, usize, &Table) {
        core.document.sections[0]
            .paragraphs
            .iter()
            .enumerate()
            .find_map(|(paragraph, para)| {
                para.controls
                    .iter()
                    .enumerate()
                    .find_map(|(control, item)| match item {
                        Control::Table(table) => Some((paragraph, control, table.as_ref())),
                        _ => None,
                    })
            })
            .expect("table control")
    }

    fn geometry(core: &DocumentCore) -> Geometry {
        let (_, _, table) = table(core);
        Geometry {
            rows: table.row_count,
            cols: table.col_count,
            width: table.common.width,
            height: table.common.height,
            cells: table
                .cells
                .iter()
                .map(|cell| {
                    (
                        cell.row,
                        cell.col,
                        cell.row_span,
                        cell.col_span,
                        cell.width,
                        cell.height,
                        cell.paragraphs
                            .iter()
                            .map(|para| para.text.as_str())
                            .collect::<Vec<_>>()
                            .join("\n"),
                    )
                })
                .collect(),
        }
    }

    fn fixture(rows: &[u32], columns: &[u32], ordinary: bool) -> DocumentCore {
        let mut core = blank();
        if ordinary {
            core.create_table_native(0, 0, 0, 2, 2)
                .expect("ordinary table");
        } else {
            core.create_table_ex_native(
                0,
                0,
                0,
                rows.len() as u16,
                columns.len() as u16,
                true,
                Some(columns),
                Some(rows),
            )
            .expect("sized table");
        }
        let (paragraph, control, table) = table(&core);
        let count = table.cells.len();
        for cell in 0..count {
            core.insert_text_in_cell_native(
                0,
                paragraph,
                control,
                cell,
                0,
                0,
                &format!("칸{cell}"),
            )
            .expect("cell text");
        }
        core
    }

    fn merge(core: &mut DocumentCore, row: u16, col: u16, end_row: u16, end_col: u16) {
        let (paragraph, control, _) = table(core);
        core.merge_table_cells_native(0, paragraph, control, row, col, end_row, end_col)
            .expect("merge cells");
    }

    // 공개 만들기·병합·속성 변경으로 만든 모델 치수를 HTML CSS에 기록한다. 가져오기 계산은 재사용하지 않는다.
    fn exact_html(core: &DocumentCore) -> String {
        let (paragraph, control, table) = table(core);
        let html = core
            .export_control_html_native(0, paragraph, &[], control)
            .expect("export HTML")
            .replacen("<table ", "<table data-rhwp-geometry=\"exact\" ", 1)
            .replacen(
                "border-collapse:collapse;",
                &format!(
                    "border-collapse:collapse;width:{:.2}pt;height:{:.2}pt;",
                    f64::from(table.common.width) / 100.0,
                    f64::from(table.common.height) / 100.0,
                ),
                1,
            );
        let mut cells: Vec<_> = table.cells.iter().collect();
        cells.sort_by_key(|cell| (cell.row, cell.col));
        let mut pieces = html.split("<td style=\"");
        let mut result = pieces.next().expect("table HTML").to_string();
        for (index, piece) in pieces.enumerate() {
            let cell = cells[index];
            result.push_str(&format!(
                "<td style=\"width:{:.2}pt;height:{:.2}pt;{piece}",
                f64::from(cell.width) / 100.0,
                f64::from(cell.height) / 100.0,
            ));
        }
        result
    }

    fn import(html: &str) -> DocumentCore {
        let mut core = blank();
        let result = core.paste_html_native(0, 0, 0, html).expect("paste HTML");
        assert!(serde_json::from_str::<serde_json::Value>(&result).unwrap()["ok"] == true);
        core
    }

    fn assert_saved_geometry(mut core: DocumentCore, expected: &Geometry) {
        assert_eq!(&geometry(&core), expected);
        let (_, _, table) = table(&core);
        assert_eq!(
            u32::from_le_bytes(
                table.raw_ctrl_data[common_obj_offsets::WIDTH]
                    .try_into()
                    .unwrap()
            ),
            expected.width,
        );
        assert_eq!(
            u32::from_le_bytes(
                table.raw_ctrl_data[common_obj_offsets::HEIGHT]
                    .try_into()
                    .unwrap()
            ),
            expected.height,
        );
        let saved = [
            core.export_hwp_with_adapter().expect("save HWP"),
            core.export_hwpx_native().expect("save HWPX"),
        ];
        for bytes in saved {
            let reopened = DocumentCore::from_bytes(&bytes).expect("reopen saved table");
            assert_eq!(&geometry(&reopened), expected);
        }
    }

    #[test]
    fn exact_css_restores_created_table_and_partial_or_complete_merges() {
        for mode in 0..5 {
            let mut source = fixture(&[3003, 4004], &[6001, 9002], false);
            match mode {
                1 => merge(&mut source, 0, 0, 0, 1),
                2 => merge(&mut source, 0, 0, 1, 0),
                3 => merge(&mut source, 0, 0, 1, 1),
                4 => {
                    merge(&mut source, 0, 0, 1, 0);
                    merge(&mut source, 0, 1, 1, 1);
                }
                _ => {}
            }
            let expected = geometry(&source);
            assert_eq!(
                (expected.rows, expected.cols, expected.height),
                (2, 2, 7007)
            );
            assert_saved_geometry(import(&exact_html(&source)), &expected);
        }
    }

    #[test]
    fn exact_css_preserves_original_height_beyond_cell_sum() {
        let source = fixture(&[], &[], true);
        let mut expected = geometry(&source);
        assert_eq!(expected.height, 2564);
        assert!(expected.cells.iter().all(|cell| cell.5 == 282));
        // 기본 표는 대상 문단 가용 폭을 넘는다. 너비 제한과 원래 세로 여유를 구분한다.
        expected.width = 40454;
        for cell in &mut expected.cells {
            cell.4 = 20227;
        }
        assert_saved_geometry(import(&exact_html(&source)), &expected);
    }

    #[test]
    fn exact_css_restores_overlapping_rowspans_without_guessing_original_tracks() {
        let mut source = fixture(&[3003, 4004, 3003, 4004, 3003, 4004], &[6001, 9002], false);
        for (row, end) in [(0, 1), (2, 3), (4, 5)] {
            merge(&mut source, row, 0, end, 0);
        }
        for (row, end) in [(0, 2), (3, 5)] {
            merge(&mut source, row, 1, end, 1);
        }
        let expected = geometry(&source);
        assert_eq!(expected.height, 21021);
        assert_eq!(
            expected.cells.iter().map(|cell| cell.5).collect::<Vec<_>>(),
            [7007, 10010, 7007, 11011, 7007],
        );
        assert_saved_geometry(import(&exact_html(&source)), &expected);
    }

    #[test]
    fn exact_css_preserves_cell_width_set_by_public_api_and_scales_only_for_available_width() {
        let mut source = fixture(&[3003, 4004], &[6001, 9002], false);
        let (paragraph, control, _) = table(&source);
        source
            .set_cell_properties_native(0, paragraph, control, 2, r#"{"width":7001}"#)
            .unwrap();
        source
            .set_cell_properties_native(0, paragraph, control, 3, r#"{"width":8002}"#)
            .unwrap();
        assert_saved_geometry(import(&exact_html(&source)), &geometry(&source));

        let wide = fixture(&[3003, 4004], &[25001, 35002], false);
        let mut expected = geometry(&wide);
        expected.width = 40454;
        for cell in &mut expected.cells {
            cell.4 = if cell.1 == 0 { 16855 } else { 23599 };
        }
        assert_saved_geometry(import(&exact_html(&wide)), &expected);

        let mut wide = fixture(&[3003, 4004], &[25001, 35002], false);
        let (paragraph, control, _) = table(&wide);
        wide.set_cell_properties_native(0, paragraph, control, 2, r#"{"width":26001}"#)
            .unwrap();
        wide.set_cell_properties_native(0, paragraph, control, 3, r#"{"width":34002}"#)
            .unwrap();
        let mut expected = geometry(&wide);
        assert_eq!(expected.width, 61003);
        expected.width = 40454;
        // 선언 폭이 넓은 행별 다른 칸은 비례 축소하고, 공유 열과 같은 칸은 기존 반올림을 쓴다.
        for (cell, width) in expected.cells.iter_mut().zip([16579, 23212, 17242, 22548]) {
            cell.4 = width;
        }
        assert_saved_geometry(import(&exact_html(&wide)), &expected);
    }

    #[test]
    fn ordinary_css_keeps_shared_columns_and_height_as_minimum() {
        let html = r#"<table style="height:5pt"><tr><td style="width:10pt;height:10pt"></td><td style="width:20pt;height:10pt"></td></tr><tr><td style="width:15pt;height:10pt"></td><td style="width:15pt;height:10pt"></td></tr></table>"#;
        let target = import(html);
        let expected = geometry(&target);
        assert_eq!((expected.width, expected.height), (3500, 2000));
        assert_eq!(
            expected.cells.iter().map(|cell| cell.4).collect::<Vec<_>>(),
            [1500, 2000, 1500, 2000]
        );
        assert_saved_geometry(target, &expected);
    }

    #[test]
    fn untrusted_exact_width_falls_back_to_shared_columns_without_losing_exact_height() {
        for html in [
            r#"<table data-rhwp-geometry="exact" style="width:10pt;height:70.07pt"><tr><td rowspan="2" style="width:1000pt;height:70.07pt">앞</td><td style="width:1000pt;height:30.03pt">위</td></tr><tr><td style="width:1000pt;height:40.04pt">아래</td></tr></table>"#,
            r#"<table data-rhwp-geometry="exact" style="width:10pt;height:70.07pt"><tr><td rowspan="2" style="width:8pt;height:70.07pt">앞</td><td style="width:2pt;height:30.03pt">위</td></tr><tr><td style="width:8pt;height:40.04pt">아래</td></tr></table>"#,
        ] {
            let target = import(html);
            let expected = geometry(&target);
            assert_eq!((expected.width, expected.height), (1000, 7007));
            assert_eq!(
                expected.cells.iter().map(|cell| cell.4).collect::<Vec<_>>(),
                [500, 500, 500]
            );
            assert_eq!(
                expected.cells.iter().map(|cell| cell.5).collect::<Vec<_>>(),
                [7007, 3003, 4004]
            );
            assert_eq!(
                expected
                    .cells
                    .iter()
                    .map(|cell| cell.6.as_str())
                    .collect::<Vec<_>>(),
                ["앞", "위", "아래"]
            );
            for row in 0..expected.rows {
                let width: u64 = expected
                    .cells
                    .iter()
                    .filter(|cell| cell.0 <= row && row < cell.0 + cell.2)
                    .map(|cell| u64::from(cell.4))
                    .sum();
                assert!(width <= u64::from(expected.width));
                assert!(width <= 40454);
            }
            assert_saved_geometry(target, &expected);
        }
    }

    #[test]
    fn ordinary_css_uses_minimum_height_for_conflicting_rowspans() {
        let html = r#"<table style="height:1pt"><tr><td rowspan="2" style="height:70pt"></td><td style="height:50pt"></td></tr><tr><td style="height:30pt"></td></tr></table>"#;
        let target = import(html);
        let expected = geometry(&target);
        assert_eq!(expected.height, 8000);
        assert_eq!(
            expected.cells.iter().map(|cell| cell.5).collect::<Vec<_>>(),
            [8000, 5000, 3000]
        );
        assert_saved_geometry(target, &expected);
    }

    #[test]
    fn colspan_skips_an_occupied_cell_in_the_middle_of_its_range() {
        let html = r#"<table><tr><td></td><td rowspan="2"></td></tr><tr><td colspan="2"></td></tr></table>"#;
        let target = import(html);
        let expected = geometry(&target);
        assert_eq!((expected.rows, expected.cols), (2, 4));
        assert_eq!(
            expected
                .cells
                .iter()
                .map(|cell| (cell.0, cell.1, cell.2, cell.3))
                .collect::<Vec<_>>(),
            [(0, 0, 1, 1), (0, 1, 2, 1), (1, 2, 1, 2)]
        );
        assert_saved_geometry(target, &expected);
    }

    #[test]
    fn invalid_or_zero_css_dimensions_fall_back_and_oversized_grids_or_heights_fail() {
        for value in ["0", "-1", "NaN", "inf", "1e308"] {
            let html = format!(
                r#"<table data-rhwp-geometry="exact" style="width:{value}pt;height:{value}pt"><tr><td style="width:{value}pt;height:{value}pt"></td></tr></table>"#
            );
            let target = import(&html);
            let shape = geometry(&target);
            assert_eq!(shape.height, 1000, "invalid size {value}");
            assert!(shape.width > 0);
            assert_saved_geometry(target, &shape);
        }
        for html in [
            r#"<table><tr><td rowspan="65535"></td></tr><tr><td rowspan="65535"></td></tr></table>"#,
            r#"<table><tr><td colspan="65535"></td><td></td></tr></table>"#,
            r#"<table><tr><td rowspan="64" colspan="65535"></td></tr></table>"#,
            r#"<table><tr><td style="height:42949672.95pt"></td></tr><tr><td style="height:42949672.95pt"></td></tr></table>"#,
        ] {
            let mut target = blank();
            let mut paragraphs = Vec::new();
            assert!(target
                .parse_table_html(&mut paragraphs, html, 42520)
                .is_err());
            assert!(paragraphs.is_empty());
        }
        assert!(html_table_tracks(1, &[(usize::MAX, 2, Some(1))], 1000).is_err());
        assert!(html_table_tracks(1, &[(0, 0, Some(1))], 1000).is_err());
    }
}

/// #4413: clipboard export가 셀 안 중첩 표를 `<table>`로 내보내도, import 쪽이 셀
/// 내용(`<td>...</td>`) 경계를 얕은(비-깊이추적) `find`로 잘라내면 안쪽 표에서
/// 잘못 멈춰 왕복이 깨진다. `<tr>` 경계 탐색에 이미 쓰는 `find_closing_tag` 깊이
/// 추적을 셀 경계에도 재사용해 중첩 `<td>`를 올바르게 건너뛰는지 검증한다.
#[cfg(test)]
mod nested_table_cell_boundary_tests {
    use crate::document_core::DocumentCore;
    use crate::model::control::Control;
    use crate::model::paragraph::Paragraph;

    /// red→green: 바깥 셀 안에 중첩 `<table>`이 있는 HTML을 파싱하면, 바깥 셀은
    /// "OUTER" 텍스트 문단과 중첩 `Control::Table`(안쪽 셀 텍스트 "INNER") 문단을
    /// 모두 포함해야 한다. 수정 전에는 셀 내용 추출이 안쪽 `</td>`에서 먼저 멈춰
    /// 바깥 셀 내용이 잘리고(중첩 표가 통째로 사라지거나 파싱이 깨졌다).
    #[test]
    fn nested_table_in_cell_round_trips_through_import() {
        let mut core = DocumentCore::new_empty();
        core.document
            .doc_info
            .char_shapes
            .push(crate::model::style::CharShape::default());
        core.document
            .doc_info
            .para_shapes
            .push(crate::model::style::ParaShape::default());
        core.document
            .doc_info
            .border_fills
            .push(crate::model::style::BorderFill::default());

        let html =
            r#"<table><tr><td>OUTER<table><tr><td>INNER</td></tr></table></td></tr></table>"#;
        let mut paragraphs: Vec<Paragraph> = Vec::new();
        core.parse_table_html(&mut paragraphs, html, 42520)
            .expect("table");

        assert_eq!(paragraphs.len(), 1, "표 문단 1개가 나와야 함");
        let outer_table = match &paragraphs[0].controls.first() {
            Some(Control::Table(t)) => t.as_ref(),
            other => panic!("Table 컨트롤이어야 함, 실제: {other:?}"),
        };
        assert_eq!(outer_table.cells.len(), 1, "바깥 표는 셀 1개");

        let outer_cell_paragraphs = &outer_table.cells[0].paragraphs;
        let has_outer_text = outer_cell_paragraphs
            .iter()
            .any(|p| p.text.contains("OUTER"));
        assert!(
            has_outer_text,
            "바깥 셀 텍스트 'OUTER'가 살아있어야 함. 실제 문단들: {outer_cell_paragraphs:?}"
        );

        let inner_table = outer_cell_paragraphs.iter().find_map(|p| {
            p.controls.iter().find_map(|c| match c {
                Control::Table(t) => Some(t.as_ref()),
                _ => None,
            })
        });
        let inner_table = inner_table.unwrap_or_else(|| {
            panic!(
                "바깥 셀 문단 중 하나에 중첩 Control::Table이 있어야 함. 실제 문단들: {outer_cell_paragraphs:?}"
            )
        });
        assert_eq!(inner_table.cells.len(), 1, "안쪽 표는 셀 1개");
        assert!(
            inner_table.cells[0]
                .paragraphs
                .iter()
                .any(|p| p.text.contains("INNER")),
            "안쪽 셀 텍스트 'INNER'가 살아있어야 함. 실제: {:?}",
            inner_table.cells[0].paragraphs
        );
    }
}
