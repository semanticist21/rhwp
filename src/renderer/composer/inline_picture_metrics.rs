//! 글자처럼 그림이 줄 구성에 제공하는 HWPUNIT 상자 크기.
//!
//! 용지·쪽 상대 비율로 확대된 인쇄 잉크와 줄 상자의 기준선 정렬 크기를 구분한다.
//! 저장 줄과 재조판이 소비하는 동일한 크기를 실제 그림 배치에도 전달한다.

use crate::model::image::Picture;

pub(crate) fn inline_picture_size_hu(picture: &Picture) -> (i32, i32) {
    crate::renderer::stored_picture_metrics::stored_picture_size_hu(picture)
}

/// 글자처럼 그림의 바깥 여백 [왼쪽, 오른쪽, 위, 아래] (HWPUNIT).
/// 한컴 12.30처럼 개체 바깥 여백, 왼쪽·오른쪽 캡션(폭 + 간격), 그림 효과(그림자·네온·반사)가 덮는
/// 만큼을 차례로 더한다: 왼쪽 캡션이면 그림이 그만큼 오른쪽으로 옮겨 가고, 오른쪽 캡션이면 뒤 글이
/// 그만큼 밀린다.
pub(crate) fn inline_picture_margins_hu(picture: &Picture) -> [i32; 4] {
    let effect = inline_picture_effect_outsets_hu(picture);
    let [left, right] = inline_side_caption_hu(picture);
    let m = &picture.common.margin;
    let margin = [
        i32::from(m.left).saturating_add(left),
        i32::from(m.right).saturating_add(right),
        i32::from(m.top),
        i32::from(m.bottom),
    ];
    std::array::from_fn(|i| margin[i].saturating_add(effect[i]))
}

/// 글자처럼 그림의 효과(그림자·네온·반사)가 그림 밖으로 덮는 [왼쪽, 오른쪽, 위, 아래] (HWPUNIT).
pub(crate) fn inline_picture_effect_outsets_hu(picture: &Picture) -> [i32; 4] {
    let (width, height) = inline_picture_size_hu(picture);
    crate::renderer::image_resolver::inline_effect_outsets_hu(picture, width, height)
}

/// 왼쪽·오른쪽 캡션이 그림 옆에 차지하는 [왼쪽, 오른쪽] 폭(캡션 폭 + 간격, HWPUNIT).
pub(crate) fn inline_side_caption_hu(picture: &Picture) -> [i32; 2] {
    use crate::model::shape::CaptionDirection;
    let Some(caption) = picture.caption.as_ref() else {
        return [0, 0];
    };
    let side = i32::try_from(caption.width)
        .unwrap_or(i32::MAX)
        .saturating_add(i32::from(caption.spacing).max(0));
    match caption.direction {
        CaptionDirection::Left => [side, 0],
        CaptionDirection::Right => [0, side],
        _ => [0, 0],
    }
}

/// 글줄이 예약하는 그림 상자는 잉크와 바깥 여백을 함께 포함한다.
/// 독립 한컴 재저장본도 위·아래 여백을 포함한 높이로 LineSeg를 발행한다.
/// 실제 잉크 배치는 기존 원점에서 같은 여백 안쪽을 소비한다.
pub(crate) fn inline_picture_occupied_size_hu(picture: &Picture) -> (i32, i32) {
    let (width, height) = inline_picture_size_hu(picture);
    let [left, right, top, bottom] = inline_picture_margins_hu(picture);
    (
        width.saturating_add(left).saturating_add(right),
        height.saturating_add(top).saturating_add(bottom),
    )
}

/// 줄 구성이 예약하는 글자처럼 그림 상자. 바깥 여백 상자에 위·아래 캡션(간격 + 캡션 높이)을
/// 더한다. 한컴 12.30: 22500×15000 그림 + 아래 캡션(간격 850, 한 줄 1000) → 줄 높이 16850.
/// 그리기는 같은 상자를 `tac_object_box_height_px`로 따로 세므로 바깥 여백에는 넣지 않는다.
/// 옆 캡션은 그림 높이에서 잘리므로 높이를 늘리지 않는다.
pub(crate) fn inline_picture_line_box_hu(picture: &Picture) -> (i32, i32) {
    use crate::model::shape::CaptionDirection;
    let (width, height) = inline_picture_occupied_size_hu(picture);
    let vertical = picture.caption.as_ref().is_some_and(|c| {
        matches!(
            c.direction,
            CaptionDirection::Top | CaptionDirection::Bottom
        )
    });
    if !vertical {
        return (width, height);
    }
    let boxed = super::captioned_shape_height_px(
        f64::from(height),
        &picture.caption,
        crate::renderer::HWPUNIT_PER_INCH,
    );
    (width, height.max(boxed.round() as i32))
}

pub(super) fn inline_picture_occupied_size_hu_with_storage<S: crate::storage::Storage>(
    picture: &Picture,
    _storage: &S,
) -> Result<(i32, i32), S::Error> {
    Ok(inline_picture_line_box_hu(picture))
}

/// HWP 5.0 표 44의 세로 정렬점. 줄 구성과 실제 그림 원점이 같은 값을 사용한다.
pub(crate) fn inline_picture_ascent_ratio(vertical_alignment: u8) -> f64 {
    match vertical_alignment {
        1 => 0.0,
        2 => 0.5,
        3 => 1.0,
        _ => 0.85,
    }
}

/// 저장 줄이 없는 혼합 주석 문단도 기존 줄 소속·탭 배치를 유지하며 그림 상자를 공유한다.
pub(super) fn refresh_picture_rows_with_storage<S: crate::storage::Storage>(
    composed: &mut super::ComposedParagraph,
    para: &crate::model::paragraph::Paragraph,
    styles: &crate::renderer::style_resolver::ResolvedStyleSet,
    storage: &S,
) -> Result<(), S::Error> {
    use crate::model::control::Control;
    use crate::renderer::{hwpunit_to_px, px_to_hwpunit, DEFAULT_DPI};
    if !para.line_segs.is_empty() {
        return Ok(());
    }
    let Some(style) = styles.para_styles.get(para.para_shape_id as usize) else {
        return Ok(());
    };
    // shortcut: 비기본 세로 정렬은 저장 줄과 주석 기준선의 별도 계약을 검증한 뒤 연결한다.
    if style.vertical_alignment != 0
        || !para.controls.iter().all(|control| match control {
            Control::Picture(picture) => picture.common.treat_as_char && picture.caption.is_none(),
            Control::Footnote(_) | Control::Endnote(_) => true,
            other => super::control_is_width_neutral_marker(other),
        })
    {
        return Ok(());
    }
    let dpi = styles.resolved_dpi.unwrap_or(DEFAULT_DPI);
    let count = composed.lines.len();
    for index in 0..count {
        let end = composed
            .lines
            .get(index + 1)
            .map_or(para.text.chars().count(), |line| line.char_start);
        let line = &mut composed.lines[index];
        let pictures: Vec<_> = storage.collect(
            composed
                .tac_controls
                .iter()
                .filter(|(position, _, _)| {
                    *position >= line.char_start
                        && (*position < end || (*position == end && index + 1 == count))
                })
                .filter_map(|(_, _, control)| match para.controls.get(*control)? {
                    Control::Picture(picture) => Some(picture),
                    _ => None,
                }),
        )?;
        if pictures.is_empty() {
            continue;
        }
        let font = crate::renderer::composed_line_max_font_size(line, para, styles);
        if !font.is_finite() || font <= 0.0 {
            continue;
        }
        let text_height = px_to_hwpunit(font, dpi);
        let mut ascent = (f64::from(text_height) * 0.85).round() as i32;
        let mut descent = text_height - ascent;
        let mut spacing_height = text_height;
        for picture in pictures {
            let height = inline_picture_occupied_size_hu(picture).1;
            let baseline = (f64::from(height)
                * inline_picture_ascent_ratio(style.vertical_alignment))
            .round() as i32;
            ascent = ascent.max(baseline);
            descent = descent.max(height - baseline);
            if picture.common.affect_line_spacing {
                spacing_height = spacing_height.max(height);
            }
        }
        if matches!(
            style.line_spacing_type,
            crate::model::style::LineSpacingType::Fixed
                | crate::model::style::LineSpacingType::Minimum
        ) {
            spacing_height = ascent + descent;
        }
        let metrics = super::frame_metrics_for_line(
            hwpunit_to_px(spacing_height, dpi),
            font,
            style.line_spacing_type,
            style.line_spacing,
            dpi,
        );
        line.line_height = ascent + descent;
        line.baseline_distance = ascent;
        line.line_spacing = metrics.line_spacing;
    }
    Ok(())
}
