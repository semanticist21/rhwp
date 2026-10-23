//! 글자 장식의 그리기·잉크 기하 — 한글 2022/2024 Print 실측 계약. (#5730)
//!
//! 종전에는 밑줄을 기준선 아래 **고정 2.0px** 에 그렸다. 한글 2022 COM 프로브
//! (9/12/15/18/24/36pt, 실선)로 실측하면 밑줄은 글꼴 크기에 비례해
//! **기준선 + 0.17em** 에 온다 (1.44/9=0.160, 2.04/12=0.170, 2.52/15=0.168,
//! 3.00/18=0.167, 4.07/24=0.170, 6.11/36=0.170). 고정 2.0px 은 11.8px 글꼴에서만
//! 우연히 맞고, 제목처럼 큰 글꼴에서는 밑줄이 디센더를 가로지른다
//! (156467175 실측: 24px 제목에서 한글 4.2px vs rhwp 0.8px).
//!
//! 이중/삼중선(shape 7~10)도 같은 프로브(15pt·24pt)로 선별 위치·굵기가 전부
//! em 비례임을 실측했다. SVG/Canvas/Skia와 clip이 이 모듈의 공통 기하를 쓴다.

/// 단일선(및 파선 등 shape 0~6, 11)의 기준선 아래 오프셋 비율 (em).
pub(crate) const UNDERLINE_BASELINE_RATIO: f64 = 0.17;

/// 한글 2024 Print는 본문 글꼴과 무관하게 이 글꼴로 강조점을 그린다.
pub(crate) const EMPHASIS_FONT_FAMILY: &str = "HCR Batang";

pub(crate) enum EmphasisMark {
    Text {
        glyph: &'static str,
        x: f64,
        y: f64,
        size: f64,
        ink: super::render_tree::BoundingBox,
    },
    Ellipse(super::render_tree::BoundingBox),
}

impl EmphasisMark {
    pub(crate) fn bounds(&self) -> super::render_tree::BoundingBox {
        match self {
            Self::Text { ink, .. } | Self::Ellipse(ink) => *ink,
        }
    }
}

/// 9/12/24/36pt와 함초롬바탕·함초롬돋움·Arial Print 실측이다.
/// 위 기호는 글자 폭의 가운데에서 0.22em 오른쪽, 기준선에서 0.37em 위에
/// 원 크기의 결합 기호를 놓는다. 공백은 제외하며 문장 부호는 포함한다.
pub(crate) fn emphasis_marks<'a>(
    text: &'a str,
    positions: &'a [f64],
    x: f64,
    baseline: f64,
    size: f64,
    kind: u8,
) -> impl Iterator<Item = EmphasisMark> + 'a {
    use super::render_tree::BoundingBox;
    // HCR Batang의 기호 잉크 상자(em). clip과 각 paint backend가 함께 쓴다.
    let glyph = match kind {
        1 => Some(("\u{0307}", [-0.342, -0.675, -0.257, -0.590])),
        2 => Some(("\u{030a}", [-0.396, -0.735, -0.204, -0.555])),
        3 => Some(("\u{030c}", [-0.452, -0.713, -0.144, -0.590])),
        4 => Some(("\u{0303}", [-0.479, -0.693, -0.156, -0.591])),
        _ => None,
    };
    text.chars()
        .zip(positions.windows(2))
        .filter(|(ch, _)| !ch.is_whitespace() && *ch != '\u{fffc}')
        .flat_map(move |(_, edges)| {
            if let Some((glyph, [left, top, right, bottom])) = glyph {
                let x = x + (edges[0] + edges[1]) / 2.0 + size * 0.22;
                let y = baseline - size * 0.37;
                [
                    Some(EmphasisMark::Text {
                        glyph,
                        x,
                        y,
                        size,
                        ink: BoundingBox::new(
                            x + size * left,
                            y + size * top,
                            size * (right - left),
                            size * (bottom - top),
                        ),
                    }),
                    None,
                ]
            } else {
                // 방점(U+302E/U+302F)은 브라우저가 홀로 놓인 기호에 점선 원을 덧붙이므로
                // 독립 Print와 같은 원을 그려, 본문 글꼴·shaper의 개입을 없앤다.
                let ellipse = |dx, cy, ry| {
                    EmphasisMark::Ellipse(BoundingBox::new(
                        x + edges[0] + size * (dx - 0.0625),
                        baseline - size * (cy + ry),
                        size * 0.125,
                        size * ry * 2.0,
                    ))
                };
                match kind {
                    5 => [Some(ellipse(-0.0595, 0.329, 0.063)), None],
                    6 => [
                        Some(ellipse(-0.0575, 0.4235, 0.0625)),
                        Some(ellipse(-0.0575, 0.1935, 0.0625)),
                    ],
                    _ => [None, None],
                }
            }
        })
        .flatten()
}

/// 위 단일선(shape 0~3)과 밑줄 다중선(shape 7~10)의 (기준선 아래 오프셋 em, 선 굵기 em) 목록.
///
/// 한글 2022 실측 (15pt/24pt 프로브, PDF 드로잉 좌표):
/// - 7 이중선: 0.160(0.032) + 0.250(0.032)
/// - 8 가는+굵은: 0.175(0.048) + 0.300(0.100)
/// - 9 굵은+가는: 0.200(0.100) + 0.325(0.048)
/// - 10 삼중선: 0.165(0.032) + 0.250(0.080) + 0.335(0.032)
///
/// 한글 2024 Print PDF(9~48pt)의 위 밑줄은 같은 획을 7번은 1.11em,
/// 8~10번은 1.20em 위로 옮긴다. 획 순서와 굵기는 그대로 유지한다.
/// 위 실선·파선·점선·일점쇄선은 기준선 -0.87em, 굵기 0.04em이다. 나머지는 기존 경로를 쓴다.
pub(crate) fn underline_lines(
    underline: crate::model::style::UnderlineType,
    shape: u8,
) -> Option<impl Iterator<Item = (f64, f64)>> {
    let lines: &[(f64, f64)] = match shape {
        0..=3 if underline == crate::model::style::UnderlineType::Top => &[(-0.87, 0.04)],
        7 => &[(0.160, 0.032), (0.250, 0.032)],
        8 => &[(0.175, 0.048), (0.300, 0.100)],
        9 => &[(0.200, 0.100), (0.325, 0.048)],
        10 => &[(0.165, 0.032), (0.250, 0.080), (0.335, 0.032)],
        _ => return None,
    };
    let lift = if underline == crate::model::style::UnderlineType::Top {
        match shape {
            7 => 1.11,
            8..=10 => 1.20,
            _ => 0.0,
        }
    } else {
        0.0
    };
    Some(lines.iter().map(move |&(dy, width)| (dy - lift, width)))
}

/// 한컴 Print의 위 파선·점선·일점쇄선은 글꼴별 run 경계에서도 획과 간격을 잇는다.
pub(crate) fn underline_spans(
    underline: crate::model::style::UnderlineType,
    shape: u8,
    width: f64,
    font_size: f64,
    offset: f64,
) -> impl Iterator<Item = (f64, f64)> {
    let dashed = underline == crate::model::style::UnderlineType::Top && matches!(shape, 1..=3);
    // 9~48pt Print 실측: 점은 600dpi 격자에 맞추고, 일점쇄선은 약 0.57em 획·0.17em 간격이다.
    const PRINT_GRID: f64 = 0.16;
    let dot_steps = (font_size * 0.057 / PRINT_GRID).round().max(1.0);
    let dot = dot_steps * PRINT_GRID;
    let (on, period) = match shape {
        2 => (dot, (dot_steps + (dot_steps * 1.5).ceil()) * PRINT_GRID),
        3 => (font_size * 0.57, font_size * 0.91 + dot),
        _ => (font_size * 0.285, font_size * 0.455),
    };
    let mut draw_dot = false;
    let mut start = if dashed {
        -offset.rem_euclid(period)
    } else {
        0.0
    };
    std::iter::from_fn(move || loop {
        if start >= width || !start.is_finite() || !width.is_finite() || (dashed && period <= 0.0) {
            return None;
        }
        let end = if dashed {
            (start + if draw_dot { dot } else { on }).min(width)
        } else {
            width
        };
        let left = start.max(0.0);
        start = if dashed && shape == 3 {
            let step = if draw_dot { dot } else { on } + font_size * 0.17;
            draw_dot = !draw_dot;
            start + step
        } else if dashed {
            start + period
        } else {
            width
        };
        if end > left {
            return Some((left, end));
        }
    })
}

/// 실선·다중선의 잉크 상하단이다. clip과 paint bounds가 같은 획을 포함한다.
pub(crate) fn underline_ink_bounds(
    underline: crate::model::style::UnderlineType,
    shape: u8,
    font_size: f64,
    baseline: f64,
) -> Option<(f64, f64)> {
    let lines = underline_lines(underline, shape)?;
    Some(lines.fold(
        (f64::INFINITY, f64::NEG_INFINITY),
        |(top, bottom), (dy, width)| {
            let y = baseline + font_size * dy;
            // SVG의 0.01px 획 반올림까지 포함한다.
            let half = (font_size * width).max(0.3) / 2.0 + 0.005;
            (top.min(y - half), bottom.max(y + half))
        },
    ))
}

/// 글줄 상자 밖으로 나가는 장식(위 밑줄·강조점)의 잉크 범위다.
///
/// 첫 줄의 이 잉크는 글줄 상자 밖, 본문 영역 위에 놓여 본문 clip에 잘린다.
/// 글자·글줄 상자를 키우면 캐럿/선택도 바뀌므로 본문 clip 계산에만 쓴다.
/// 장식의 잉크는 각 paint backend와 같은 기하로 계산한다.
/// 회전·세로쓰기·글자겹침은 다루지 않는다.
pub(crate) fn raised_decoration_ink_bbox(
    bbox: super::render_tree::BoundingBox,
    run: &super::render_tree::TextRunNode,
) -> Option<super::render_tree::BoundingBox> {
    use crate::model::style::UnderlineType;

    let style = &run.style;
    let top_underline = style.underline == UnderlineType::Top && style.underline_shape <= 10;
    let emphasis = (1..=6).contains(&style.emphasis_dot);
    if !(top_underline || emphasis)
        || run.char_overlap.is_some()
        || run.rotation != 0.0
        || run.is_vertical
        || bbox.width <= 0.0
        || !run.display_or_text().chars().any(|ch| ch != '\u{FFFC}')
    {
        return None;
    }
    let base_size = if style.font_size > 0.0 {
        style.font_size
    } else {
        12.0
    };
    let (font_size, baseline) = style.script_draw_metrics(base_size, bbox.y + run.baseline);
    let mut top = f64::INFINITY;
    let mut bottom = f64::NEG_INFINITY;
    let mut left = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    if top_underline {
        if let Some((ink_top, ink_bottom)) =
            underline_ink_bounds(style.underline, style.underline_shape, font_size, baseline)
        {
            top = top.min(ink_top);
            bottom = bottom.max(ink_bottom);
        } else {
            let y = baseline - font_size + 1.0;
            top = top.min(y - 0.5);
            bottom = bottom.max(y + 0.5);
        }
        let end_pad = if style.underline_shape == 6 { 0.5 } else { 0.0 };
        left = bbox.x - end_pad;
        right = bbox.x + bbox.width + end_pad;
    }
    if emphasis {
        let text = run.display_or_text();
        let positions = run.replay_positions_for(text);
        for mark in emphasis_marks(
            text,
            &positions,
            bbox.x,
            baseline,
            font_size,
            style.emphasis_dot,
        ) {
            let ink = mark.bounds();
            // 래스터의 반 픽셀 안티앨리어싱까지 포함한다.
            left = left.min(ink.x - 0.5);
            right = right.max(ink.x + ink.width + 0.5);
            top = top.min(ink.y - 0.5);
            bottom = bottom.max(ink.y + ink.height + 0.5);
        }
    }
    let ink = super::render_tree::BoundingBox::new(left, top, right - left, bottom - top);
    [ink.x, ink.y, ink.width, ink.height]
        .iter()
        .all(|v| v.is_finite())
        .then_some(ink)
}
