//! HWP 도형 그림자와 HWPX의 공식 이름 대응.

const HWPX_NAMES: [&str; 15] = [
    "NONE",
    "PARELLEL_LEFTTOP",
    "PARELLEL_RIGHTTOP",
    "PARELLEL_LEFTBOTTOM",
    "PARELLEL_RIGHTBOTTOM",
    "SHEAR_LEFTTOP",
    "SHEAR_RIGHTTOP",
    "SHEAR_LEFTBOTTOM",
    "SHEAR_RIGHTBOTTOM",
    "PERS_LEFTTOP",
    "PERS_RIGHTTOP",
    "PERS_LEFTBOTTOM",
    "PERS_RIGHTBOTTOM",
    "SCALE_NARROW",
    "SCALE_ENLARGE",
];

pub(crate) fn from_hwpx(name: &str) -> u32 {
    HWPX_NAMES
        .iter()
        .position(|candidate| *candidate == name)
        .map(|index| index as u32)
        .unwrap_or_else(|| match name {
            // 공식 모델의 이전 버전 읽기 별칭과 과거 rhwp 저장본을 수용한다.
            "DROP" | "LEFT_TOP" => 1,
            "CONTINUOUS" | "RIGHT_TOP" => 2,
            "LEFT_BOTTOM" => 3,
            "RIGHT_BOTTOM" => 4,
            "CENTER" | "INSIDE" | "OUTSIDE" => 5,
            _ => 0,
        })
}

pub(crate) fn to_hwpx(kind: u32) -> &'static str {
    HWPX_NAMES.get(kind as usize).copied().unwrap_or("NONE")
}

/// 평행·작게·크게 그림자의 기본 이동량(HWPUNIT). 저장 offset은 이 값에 부호를 보존해 더한다.
const PRESET_HU: f64 = 600.0;
/// 한/글 12.30 화면에서 잰 작게·크게 그림자의 배율.
const NARROW_SCALE: f64 = 0.75;
const ENLARGE_SCALE: f64 = 1.25;
/// 기울임 그림자는 본체 높이의 반이고, 먼 변이 본체 높이의 0.56756배만큼 옆으로 밀린다.
/// 기울기는 한/글 12.30이 HWP 바깥 여백에 적은 그림자 몫(높이 4000·8000·10000 → 2270·4540·5676,
/// 기울인 사각형 1628·4328)을 모두 반올림으로 맞추는 값이다.
const SHEAR_HEIGHT: f64 = 0.5;
const SHEAR_SLOPE: f64 = 0.56756;

/// 평행 그림자의 기본 이동량. 저장 offset은 이 값에 부호를 보존해 더한다.
pub(crate) fn parallel_preset_hu(kind: u32) -> Option<(i32, i32)> {
    match kind {
        1 => Some((-600, -600)),
        2 => Some((600, -600)),
        3 => Some((-600, 600)),
        4 => Some((600, 600)),
        _ => None,
    }
}

/// 본체 상자(x, y, w, h) 위의 점을 그림자 자리로 옮기는 아핀 행렬 `[a, b, c, d, e, f]`
/// (x' = a·x + c·y + e, y' = b·x + d·y + f). `unit`은 HWPUNIT 하나의 길이, `offset`은 저장 offset이다.
/// 평행은 이동, 작게·크게는 왼쪽 위·오른쪽 아래 모서리를 기준으로 줄이고 늘린 뒤 왼쪽 위로 옮기며,
/// 기울임은 아래 변에서 위(뒤쪽 그림자) 또는 아래(앞쪽 그림자, 상하로 뒤집힘)로 반 높이만큼 눕힌다.
/// 원근(9~12)은 한/글 Mac에서 고를 수 없고 단순 이동으로 대체하지 않으므로 None이다.
pub fn matrix(
    kind: u32,
    [x, y, w, h]: [f64; 4],
    unit: f64,
    (offset_x, offset_y): (f64, f64),
) -> Option<[f64; 6]> {
    let d = PRESET_HU * unit;
    let bottom = y + h;
    let m = match kind {
        1..=4 => {
            let (px, py) = parallel_preset_hu(kind)?;
            [
                1.0,
                0.0,
                0.0,
                1.0,
                f64::from(px) * unit,
                f64::from(py) * unit,
            ]
        }
        13 | 14 => {
            let (s, ax, ay) = if kind == 13 {
                (NARROW_SCALE, x, y)
            } else {
                (ENLARGE_SCALE, x + w, bottom)
            };
            [s, 0.0, 0.0, s, ax - s * ax - d, ay - s * ay - d]
        }
        5..=8 => {
            // 5 왼쪽 위, 6 오른쪽 위, 7 왼쪽 아래, 8 오른쪽 아래
            let c = if matches!(kind, 5 | 7) {
                SHEAR_SLOPE
            } else {
                -SHEAR_SLOPE
            };
            let dy = if kind <= 6 {
                SHEAR_HEIGHT
            } else {
                -SHEAR_HEIGHT
            };
            [1.0, 0.0, c, dy, -c * bottom, bottom - dy * bottom]
        }
        _ => return None,
    };
    let m = [m[0], m[1], m[2], m[3], m[4] + offset_x, m[5] + offset_y];
    m.iter().all(|v| v.is_finite()).then_some(m)
}

/// 행렬로 옮긴 상자의 외곽(x, y, w, h).
pub fn transformed_box(m: [f64; 6], [x, y, w, h]: [f64; 4]) -> [f64; 4] {
    let corners = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)]
        .map(|(px, py)| (m[0] * px + m[2] * py + m[4], m[1] * px + m[3] * py + m[5]));
    let min_x = corners.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let min_y = corners.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let max_x = corners
        .iter()
        .map(|p| p.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let max_y = corners
        .iter()
        .map(|p| p.1)
        .fold(f64::NEG_INFINITY, f64::max);
    [min_x, min_y, max_x - min_x, max_y - min_y]
}

/// 틀(0, 0, w, h) 안 본체 외곽 꼭짓점에 그림자를 걸었을 때 틀 밖으로 나가는 몫
/// `[왼쪽, 위, 오른쪽, 아래]`(HWPUNIT). `outline`이 없으면 틀 네 모서리를 외곽으로 본다.
/// 기울인 도형은 한/글 12.30처럼 기울인 외곽에서 잰다(틀은 그 외접 상자다).
pub fn extents_hu(
    kind: u32,
    (x, y): (i32, i32),
    w: f64,
    h: f64,
    outline: Option<&[(f64, f64); 4]>,
) -> Option<[f64; 4]> {
    let m = matrix(kind, [0.0, 0.0, w, h], 1.0, (f64::from(x), f64::from(y)))?;
    let corners = outline
        .copied()
        .unwrap_or([(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)]);
    let points =
        corners.map(|(px, py)| (m[0] * px + m[2] * py + m[4], m[1] * px + m[3] * py + m[5]));
    let min_x = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let min_y = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let max_x = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    let max_y = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    Some([
        (-min_x).max(0.0),
        (-min_y).max(0.0),
        (max_x - w).max(0.0),
        (max_y - h).max(0.0),
    ])
}

/// 독립 개체의 선언 원점은 본체와 그림자를 합친 외곽의 왼쪽 위다(한/글 12.30 화면에서 확인).
/// 본체를 원점에서 얼마나 밀어야 하는지(HWPUNIT) 돌려준다.
/// 묶음 자식의 renderingInfo 원점과는 별개의 배치 계약이다.
pub fn body_origin_hu(kind: u32, x: i32, y: i32, w: f64, h: f64) -> Option<(f64, f64)> {
    extents_hu(kind, (x, y), w, h, None).map(|[left, top, _, _]| (left, top))
}

/// 기울이기만 저장한 독립 도형(사각형·타원·호·곡선, 회전·대칭 없음)의 틀 행렬 `[a, b, c, d, e, f]`
/// (x' = a·x + c·y + e, HWPUNIT). 원래 크기 좌표를 틀(외접 상자) 안으로 옮긴다.
/// 다각형은 꼭짓점에 저장 행렬을 이미 적용하므로 없다.
pub fn frame_skew_hu(shape: &super::ShapeObject) -> Option<[f64; 6]> {
    use super::ShapeObject;
    if !matches!(
        shape,
        ShapeObject::Rectangle(_)
            | ShapeObject::Ellipse(_)
            | ShapeObject::Arc(_)
            | ShapeObject::Curve(_)
    ) {
        return None;
    }
    let sa = shape.shape_attr();
    if sa.rotation_angle.rem_euclid(360) != 0
        || sa.horz_flip
        || sa.vert_flip
        || (sa.render_b.abs() <= 1e-6 && sa.render_c.abs() <= 1e-6)
        || sa.original_width == 0
        || sa.original_height == 0
        || !sa.has_saved_affine_geometry()
    {
        return None;
    }
    Some([
        sa.render_sx,
        sa.render_c,
        sa.render_b,
        sa.render_sy,
        sa.render_tx,
        sa.render_ty,
    ])
}

/// 그림자를 재는 본체 외곽(틀 왼쪽 위 기준 HWPUNIT). 기울인 도형만 있고, 나머지는 틀 네 모서리다.
pub fn skewed_outline_hu(shape: &super::ShapeObject) -> Option<[(f64, f64); 4]> {
    let m = frame_skew_hu(shape)?;
    let sa = shape.shape_attr();
    let (w, h) = (f64::from(sa.original_width), f64::from(sa.original_height));
    Some(
        [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)]
            .map(|(px, py)| (m[0] * px + m[2] * py + m[4], m[1] * px + m[3] * py + m[5])),
    )
}

/// 한/글 12.30이 HWP 개체 바깥 여백 칸에 더해 적는 그림자 몫 `[왼쪽, 오른쪽, 위, 아래]`(HWPUNIT, 반올림).
/// HWPX `outMargin`에는 더하지 않는다. 그림자가 없거나 그리지 않는 종류면 없다.
pub fn hwp_margin_extents(shape: &super::ShapeObject) -> Option<[i16; 4]> {
    use super::ShapeObject;
    // 그림 효과의 그림자와 묶음·차트·OLE는 이 계약 밖이다(한/글 저장본으로 확인한 것은 그리기 도형뿐이다).
    if !matches!(
        shape,
        ShapeObject::Line(_)
            | ShapeObject::Rectangle(_)
            | ShapeObject::Ellipse(_)
            | ShapeObject::Arc(_)
            | ShapeObject::Polygon(_)
            | ShapeObject::Curve(_)
    ) {
        return None;
    }
    let drawing = shape.drawing()?;
    let common = shape.common();
    let outline = skewed_outline_hu(shape);
    let [left, top, right, bottom] = extents_hu(
        drawing.shadow_type,
        (drawing.shadow_offset_x, drawing.shadow_offset_y),
        f64::from(common.width),
        f64::from(common.height),
        outline.as_ref(),
    )?;
    let hu = |v: f64| v.round().min(f64::from(i16::MAX)) as i16;
    let extents = [hu(left), hu(right), hu(top), hu(bottom)];
    extents.iter().any(|&v| v != 0).then_some(extents)
}

/// 그림자 몫을 뺀 바깥 여백(HWPX `outMargin`과 개체 속성 창이 쓰는 값).
pub fn clean_margin(common: &super::CommonObjAttr) -> crate::model::Padding {
    let margin = common.margin;
    let Some([left, right, top, bottom]) = common.hwp_shadow_margin else {
        return margin;
    };
    let strip = |value: i16, extent: i16| {
        if extent == 0 {
            value
        } else {
            value.saturating_sub(extent).max(0)
        }
    };
    crate::model::Padding {
        left: strip(margin.left, left),
        right: strip(margin.right, right),
        top: strip(margin.top, top),
        bottom: strip(margin.bottom, bottom),
    }
}

/// HWP로 쓸 바깥 여백. HWP에서 읽은 그대로가 아니면 지금 그림자 몫을 더한다.
pub fn hwp_margin(shape: &super::ShapeObject) -> crate::model::Padding {
    let common = shape.common();
    let margin = common.margin;
    if common.hwp_shadow_margin.is_some() {
        return margin;
    }
    let Some([left, right, top, bottom]) = hwp_margin_extents(shape) else {
        return margin;
    };
    crate::model::Padding {
        left: margin.left.saturating_add(left),
        right: margin.right.saturating_add(right),
        top: margin.top.saturating_add(top),
        bottom: margin.bottom.saturating_add(bottom),
    }
}

/// HWP에서 읽은 도형이 바깥 여백에 그림자 몫을 담고 있으면 그 몫을 적어 둔다.
/// 조판은 지금처럼 읽은 여백을 쓰고, HWPX·개체 속성은 몫을 뺀 값을, HWP 저장은 읽은 값을 쓴다.
/// 몫보다 작은 여백(그림자 몫을 적지 않은 예전 rhwp 저장본)은 몫이 없는 여백으로 본다.
pub fn record_hwp_margin(shape: &mut super::ShapeObject) {
    let Some(extents) = hwp_margin_extents(shape) else {
        return;
    };
    let common = shape.common_mut();
    let m = common.margin;
    let [left, right, top, bottom] = extents;
    if m.left >= left && m.right >= right && m.top >= top && m.bottom >= bottom {
        common.hwp_shadow_margin = Some(extents);
    }
}

/// 여백을 그림자 몫 없는 값으로 고친다. 그림자·크기를 고치기 전에 불러 HWP 저장이 새 몫을 다시 더하게 한다.
pub fn normalize_margin(common: &mut super::CommonObjAttr) {
    if common.hwp_shadow_margin.is_some() {
        common.margin = clean_margin(common);
        common.hwp_shadow_margin = None;
    }
}
