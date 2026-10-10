//! 글자 장식(밑줄) 기하 — 한글 2022 실측 계약. (#5730)
//!
//! 종전에는 밑줄을 기준선 아래 **고정 2.0px** 에 그렸다. 한글 2022 COM 프로브
//! (9/12/15/18/24/36pt, 실선)로 실측하면 밑줄은 글꼴 크기에 비례해
//! **기준선 + 0.17em** 에 온다 (1.44/9=0.160, 2.04/12=0.170, 2.52/15=0.168,
//! 3.00/18=0.167, 4.07/24=0.170, 6.11/36=0.170). 고정 2.0px 은 11.8px 글꼴에서만
//! 우연히 맞고, 제목처럼 큰 글꼴에서는 밑줄이 디센더를 가로지른다
//! (156467175 실측: 24px 제목에서 한글 4.2px vs rhwp 0.8px).
//!
//! 이중/삼중선(shape 7~10)도 같은 프로브(15pt·24pt)로 선별 위치·굵기가 전부
//! em 비례임을 실측했다. 이 모듈이 그 계약의 단일 출처다 — SVG/Skia 백엔드가
//! 같은 표를 소비한다.

/// 단일선(및 파선 등 shape 0~6, 11)의 기준선 아래 오프셋 비율 (em).
pub(crate) const UNDERLINE_BASELINE_RATIO: f64 = 0.17;

/// 밑줄 다중선(shape 7~10)의 (기준선 아래 오프셋 em, 선 굵기 em) 목록.
///
/// 한글 2022 실측 (15pt/24pt 프로브, PDF 드로잉 좌표):
/// - 7 이중선: 0.160(0.032) + 0.250(0.032)
/// - 8 가는+굵은: 0.175(0.048) + 0.300(0.100)
/// - 9 굵은+가는: 0.200(0.100) + 0.325(0.048)
/// - 10 삼중선: 0.165(0.032) + 0.250(0.080) + 0.335(0.032)
///
/// 그 외 shape 는 `None` — 호출부가 단일선 경로(0.17em, 기존 선 모양 유지)로 그린다.
pub(crate) fn underline_multi_lines(shape: u8) -> Option<&'static [(f64, f64)]> {
    match shape {
        7 => Some(&[(0.160, 0.032), (0.250, 0.032)]),
        8 => Some(&[(0.175, 0.048), (0.300, 0.100)]),
        9 => Some(&[(0.200, 0.100), (0.325, 0.048)]),
        10 => Some(&[(0.165, 0.032), (0.250, 0.080), (0.335, 0.032)]),
        _ => None,
    }
}

/// 가로 위 밑줄의 실제 잉크 범위다. 글자·글줄 상자는 바꾸지 않고 본문 clip만 넓힌다.
/// 아래 밑줄의 em 표와 달리 위 밑줄은 기존 draw_line_shape의 고정 획 기하를 쓴다.
/// 물결·회전·세로쓰기·글자겹침은 이 가로선 계약에 포함하지 않는다.
pub(crate) fn top_underline_ink_bbox(
    bbox: super::render_tree::BoundingBox,
    run: &super::render_tree::TextRunNode,
) -> Option<super::render_tree::BoundingBox> {
    use super::render_tree::BoundingBox;
    use crate::model::style::UnderlineType;

    if run.style.underline != UnderlineType::Top
        || run.style.underline_shape > 10
        || run.char_overlap.is_some()
        || run.rotation != 0.0
        || run.is_vertical
        || !run.display_or_text().chars().any(|ch| ch != '\u{FFFC}')
        || bbox.width <= 0.0
        || bbox.height < 0.0
        || ![
            bbox.x,
            bbox.y,
            bbox.width,
            bbox.height,
            run.baseline,
            run.style.font_size,
        ]
        .iter()
        .all(|value| value.is_finite())
    {
        return None;
    }
    let base_size = if run.style.font_size > 0.0 {
        run.style.font_size
    } else {
        12.0
    };
    let (font_size, baseline) = run
        .style
        .script_draw_metrics(base_size, bbox.y + run.baseline);
    // SVG/Skia의 기존 위 밑줄 위치다. 장평 축소 Canvas 선은 이보다 아래에 있어
    // 원래 글줄 상자와 이 범위의 합집합 안에 들어간다.
    let y = baseline - font_size + 1.0;
    let (top, bottom) = match run.style.underline_shape {
        7 => (-1.35, 1.35),
        8 => (-1.45, 1.4),
        9 => (-1.4, 1.45),
        10 => (-1.75, 1.75),
        _ => (-0.5, 0.5),
    };
    // 원형 점선만 round cap으로 선의 양 끝이 획 반만큼 번진다.
    let end_pad = if run.style.underline_shape == 6 {
        0.5
    } else {
        0.0
    };
    let ink = BoundingBox::new(
        bbox.x - end_pad,
        y + top,
        bbox.width + 2.0 * end_pad,
        bottom - top,
    );
    [
        ink.x,
        ink.y,
        ink.width,
        ink.height,
        ink.x + ink.width,
        ink.y + ink.height,
    ]
    .iter()
    .all(|value| value.is_finite())
    .then_some(ink)
}
