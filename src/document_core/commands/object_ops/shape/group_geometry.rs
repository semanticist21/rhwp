//! 묶음 경계와 자식 저장 행렬이 함께 사용하는 원본 → 현재 프레임 변환.

use crate::model::shape::ShapeObject;

/// 최상위 도형의 표시 중심은 유지하고, 저장 프레임만 회전 후 외곽 크기로 맞춘다.
/// 원본 꼭짓점과 current 크기는 회전 전 로컬 좌표계이므로 변경하지 않는다.
pub(super) fn refresh_top_level_frame(shape: &mut ShapeObject) {
    use crate::model::shape::{HorzAlign, VertAlign};

    let sa = shape.shape_attr();
    let (w, h) = (sa.current_width, sa.current_height);
    let (bw, bh) =
        crate::document_core::DocumentCore::picture_rotated_bounds(w, h, sa.rotation_angle);
    if w == 0 || h == 0 {
        return;
    }
    let common = shape.common_mut();
    // 가운데 정렬은 프레임 크기 변화가 이미 정렬 원점에 반영된다.
    if common.horz_align != HorzAlign::Center {
        let delta = (i64::from(common.width) - i64::from(bw)) / 2;
        common.horizontal_offset =
            (i64::from(common.horizontal_offset as i32) + delta) as i32 as u32;
    }
    if common.vert_align != VertAlign::Center {
        let delta = (i64::from(common.height) - i64::from(bh)) / 2;
        common.vertical_offset = (i64::from(common.vertical_offset as i32) + delta) as i32 as u32;
    }
    common.width = bw;
    common.height = bh;
    let sa = shape.shape_attr_mut();
    sa.rotation_center.x = (bw / 2) as i32;
    sa.rotation_center.y = (bh / 2) as i32;
}

/// 크기·회전·대칭 변경에서만 캐시를 비우며, 위치 이동은 원본 로컬 행렬을 보존한다.
pub(super) fn finish_top_level_edit(
    shape: &mut ShapeObject,
    before: (u32, u32, u32, u32, u32, u32, i16, bool, bool),
) {
    if matches!(shape, ShapeObject::Group(_)) {
        return;
    }
    if let ShapeObject::Line(line) = shape {
        finish_line_edit(line, before);
        return;
    }
    if shape.shape_attr().rotation_angle != before.6 {
        refresh_top_level_frame(shape);
    }
    // 다각형은 공통 affine 편집기가 원본 행렬의 잔여 변환을 보존한다.
    // 먼저 행렬을 초기화하면 원본의 skew·이동을 복구할 수 없다.
    if matches!(shape, ShapeObject::Polygon(_)) {
        return;
    }
    let after =
        super::super::common::shape_transform_fingerprint(shape.common(), shape.shape_attr());
    // common offset은 문단/셀의 앵커 이동이다. 로컬 행렬의 이동·배율까지 지우면
    // 도형이 다른 위치로 뛰고, offset만 되돌리는 Undo로 원래 기하를 복원할 수 없다.
    let geometry =
        |f: (u32, u32, u32, u32, u32, u32, i16, bool, bool)| (f.0, f.1, f.4, f.5, f.6, f.7, f.8);
    if geometry(after) != geometry(before) {
        let sa = shape.shape_attr_mut();
        sa.raw_rendering.clear();
        sa.render_tx = 0.0;
        sa.render_ty = 0.0;
        sa.render_sx = 1.0;
        sa.render_sy = 1.0;
        sa.render_b = 0.0;
        sa.render_c = 0.0;
    }
}

/// 직선의 크기·회전·대칭 편집. 한컴 12.30은 직선을 돌리면 개체 자리(틀 왼쪽 위)를 두고 틀을
/// 돌린 외접 상자로 바꾸며, 끝점은 그대로 두고 회전·대칭을 저장 행렬(회전 행렬)에 적는다.
fn finish_line_edit(
    line: &mut crate::model::shape::LineShape,
    before: (u32, u32, u32, u32, u32, u32, i16, bool, bool),
) {
    let sa = &line.drawing.shape_attr;
    let after = (
        line.common.width,
        line.common.height,
        sa.current_width,
        sa.current_height,
        sa.rotation_angle,
        sa.horz_flip,
        sa.vert_flip,
    );
    if after
        == (
            before.0, before.1, before.4, before.5, before.6, before.7, before.8,
        )
    {
        return;
    }
    let (w, h) = (f64::from(sa.current_width), f64::from(sa.current_height));
    let (sin, cos) = f64::from(sa.rotation_angle).to_radians().sin_cos();
    if sa.rotation_angle != before.6 {
        line.common.width = (w * cos.abs() + h * sin.abs()).round() as u32;
        line.common.height = (w * sin.abs() + h * cos.abs()).round() as u32;
    }
    let (frame_w, frame_h) = (f64::from(line.common.width), f64::from(line.common.height));
    let sa = &mut line.drawing.shape_attr;
    sa.rotation_center.x = (line.common.width / 2) as i32;
    sa.rotation_center.y = (line.common.height / 2) as i32;
    if sa.rotation_angle.rem_euclid(360) == 0 && !sa.horz_flip && !sa.vert_flip {
        sa.raw_rendering.clear();
        sa.render_tx = 0.0;
        sa.render_ty = 0.0;
        sa.render_sx = 1.0;
        sa.render_sy = 1.0;
        sa.render_b = 0.0;
        sa.render_c = 0.0;
        return;
    }
    let ratio = |current: u32, original: u32| {
        if current > 0 && original > 0 {
            f64::from(current) / f64::from(original)
        } else {
            1.0
        }
    };
    let (kx, ky) = (
        ratio(sa.current_width, sa.original_width),
        ratio(sa.current_height, sa.original_height),
    );
    let fx = if sa.horz_flip { -1.0 } else { 1.0 };
    let fy = if sa.vert_flip { -1.0 } else { 1.0 };
    // 현재 크기 상자의 가운데를 틀 가운데에 두고 대칭 → 회전한다. 크기 0인 축(가로·세로 선)도 그대로 둔다.
    let (hx, hy) = (fx * w / 2.0, fy * h / 2.0);
    let rotation = [cos * fx, -sin * fy, 0.0, sin * fx, cos * fy, 0.0];
    let (tx, ty) = (
        frame_w / 2.0 - (cos * hx - sin * hy),
        frame_h / 2.0 - (sin * hx + cos * hy),
    );
    sa.render_sx = rotation[0] * kx;
    sa.render_b = rotation[1] * ky;
    sa.render_tx = tx;
    sa.render_c = rotation[3] * kx;
    sa.render_sy = rotation[4] * ky;
    sa.render_ty = ty;
    // 한컴 저장본처럼 이동·크기·회전(대칭 포함) 행렬로 나눠 적는다. 합성은 이동 × 회전 × 크기다.
    let mut raw = Vec::with_capacity(2 + 8 * 6 * (3 + 2 * sa.group_level as usize));
    raw.extend_from_slice(&(sa.group_level + 1).to_le_bytes());
    let identity = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let matrices = [
        [1.0, 0.0, tx, 0.0, 1.0, ty],
        [kx, 0.0, 0.0, 0.0, ky, 0.0],
        rotation,
    ];
    for value in matrices
        .iter()
        .chain(std::iter::repeat_n(&identity, 2 * sa.group_level as usize))
        .flatten()
    {
        raw.extend_from_slice(&value.to_le_bytes());
    }
    sa.raw_rendering = raw;
}

pub(super) struct Placement {
    pub translation: (f64, f64),
    pub scale: [f64; 6],
    pub rotation: [f64; 6],
    original_size: (f64, f64),
}

impl Placement {
    pub(super) fn bounds(&self) -> (f64, f64, f64, f64) {
        let (w, h) = self.original_size;
        let [sx, _, _, _, sy, _] = self.scale;
        let [a, b, _, c, d, _] = self.rotation;
        [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]
            .into_iter()
            .map(|(x, y)| {
                (
                    a * sx * x + b * sy * y + self.translation.0,
                    c * sx * x + d * sy * y + self.translation.1,
                )
            })
            .fold(
                (
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                ),
                |(left, top, right, bottom), (x, y)| {
                    (left.min(x), top.min(y), right.max(x), bottom.max(y))
                },
            )
    }
}

/// 최상위 프레임 중심을 유지하며 원본 좌표에 크기·대칭·회전을 차례로 적용한다.
pub(super) fn placement(child: &ShapeObject, x: f64, y: f64) -> Placement {
    placement_from_attributes(child.common(), child.shape_attr(), x, y)
}

fn placement_from_attributes(
    common: &crate::model::shape::CommonObjAttr,
    sa: &crate::model::shape::ShapeComponentAttr,
    x: f64,
    y: f64,
) -> Placement {
    let (frame_w, frame_h) = (common.width as f64, common.height as f64);
    let positive = |value: u32, fallback: f64| if value > 0 { value as f64 } else { fallback };
    let cur_w = positive(sa.current_width, frame_w);
    let cur_h = positive(sa.current_height, frame_h);
    let original_w = positive(sa.original_width, cur_w);
    let original_h = positive(sa.original_height, cur_h);
    // 수평·수직 선의 길이가 없는 축은 단위 배율로 보존한다.
    let scale_x = if original_w > 0.0 {
        cur_w / original_w
    } else {
        1.0
    };
    let scale_y = if original_h > 0.0 {
        cur_h / original_h
    } else {
        1.0
    };
    let fx = if sa.horz_flip { -1.0 } else { 1.0 };
    let fy = if sa.vert_flip { -1.0 } else { 1.0 };
    let (sin, cos) = (sa.rotation_angle as f64).to_radians().sin_cos();
    let (hx, hy) = (fx * cur_w / 2.0, fy * cur_h / 2.0);
    Placement {
        translation: (
            x + frame_w / 2.0 - (cos * hx - sin * hy),
            y + frame_h / 2.0 - (sin * hx + cos * hy),
        ),
        scale: [fx * scale_x, 0.0, 0.0, 0.0, fy * scale_y, 0.0],
        rotation: [cos, -sin, 0.0, sin, cos, 0.0],
        original_size: (original_w, original_h),
    }
}

/// 배치 프레임과 저장 행렬이 같은 변환을 소비한다.
pub(in crate::document_core::commands::object_ops) fn frame_matrix(
    common: &crate::model::shape::CommonObjAttr,
    attr: &crate::model::shape::ShapeComponentAttr,
) -> [f64; 6] {
    let placement = placement_from_attributes(common, attr, 0.0, 0.0);
    let [sx, _, _, _, sy, _] = placement.scale;
    let [a, b, _, c, d, _] = placement.rotation;
    [
        a * sx,
        b * sy,
        placement.translation.0,
        c * sx,
        d * sy,
        placement.translation.1,
    ]
}

type Fingerprint = (u32, u32, u32, u32, u32, u32, i16, bool, bool);

/// 묶음 속성 변경의 뒷정리. 크기가 바뀌면 한컴 12.30처럼 묶음 자신의 행렬과 모든 자손의
/// 최상위 묶음 단계 행렬에 배율을 합성해 자식이 묶음 상자를 따라 늘거나 준다.
pub(super) fn finish_group_edit(
    group: &mut crate::model::shape::GroupShape,
    before: Fingerprint,
    new_w: Option<u32>,
    new_h: Option<u32>,
    props_json: &str,
) {
    if rotate_group(group, before, props_json) {
        return;
    }
    let sa = &mut group.shape_attr;
    let (old_w, old_h) = (sa.current_width, sa.current_height);
    // [#6806] 값이 바뀔 때만 — 파싱값이 `current ≠ common` 인 묶음은 같은 봉지를 되먹여도
    // 지문이 흔들리면 안 된다.
    if let Some(nw) = new_w.filter(|&w| w != before.0) {
        sa.current_width = nw;
    }
    if let Some(nh) = new_h.filter(|&h| h != before.1) {
        sa.current_height = nh;
    }
    let ratio = |new: u32, old: u32| {
        if new != old && old > 0 {
            new as f64 / old as f64
        } else {
            1.0
        }
    };
    let (kx, ky) = (
        ratio(sa.current_width, old_w),
        ratio(sa.current_height, old_h),
    );
    // 회전 중심 갱신 — common 에서 다시 세우므로 무변경 시 멱등이다.
    sa.rotation_center.x = (group.common.width / 2) as i32;
    sa.rotation_center.y = (group.common.height / 2) as i32;
    // [#6740] raw_rendering 초기화는 실제로 변환이 바뀐 뒤에만 한다.
    if super::super::common::shape_transform_fingerprint(&group.common, sa) != before {
        sa.raw_rendering = Vec::new();
    }
    if kx == 1.0 && ky == 1.0 {
        return;
    }
    // 한컴 저장본처럼 묶음 자신의 크기 행렬은 current/original 배율이 된다(원본 행렬은 위에서 비웠다).
    scale_rendering(sa, kx, ky);
    scale_descendants(&mut group.children, kx, ky);
}

/// 묶음 회전·대칭. 한컴 12.30은 개체 자리(틀 왼쪽 위)를 두고 틀을 돌린 외접 상자로 바꾸며,
/// 옛 틀에서 새 틀로 가는 변환 G를 묶음과 모든 자손의 저장 행렬 앞에 합성한다(자손은 이동과 첫
/// 묶음 단계 회전 행렬에, 이동은 틀 가운데가 옮긴 만큼). 값이 바뀌지 않으면 false다.
/// 같은 편집의 크기 값은 쓰지 않는다.
fn rotate_group(
    group: &mut crate::model::shape::GroupShape,
    before: Fingerprint,
    props_json: &str,
) -> bool {
    use crate::document_core::helpers::{json_bool, json_i32};
    let sa = &mut group.shape_attr;
    if let Some(angle) = json_i32(props_json, "rotationAngle") {
        sa.rotation_angle = angle as i16;
    }
    if let Some(on) = json_bool(props_json, "horzFlip") {
        sa.horz_flip = on;
    }
    if let Some(on) = json_bool(props_json, "vertFlip") {
        sa.vert_flip = on;
    }
    sa.flip = (sa.flip & !3) | u32::from(sa.horz_flip) | (u32::from(sa.vert_flip) << 1);
    if (sa.rotation_angle, sa.horz_flip, sa.vert_flip) == (before.6, before.7, before.8) {
        return false;
    }
    let old_frame = frame_matrix(
        &crate::model::shape::CommonObjAttr {
            width: before.0,
            height: before.1,
            ..Default::default()
        },
        &crate::model::shape::ShapeComponentAttr {
            original_width: sa.original_width,
            original_height: sa.original_height,
            current_width: before.4,
            current_height: before.5,
            rotation_angle: before.6,
            horz_flip: before.7,
            vert_flip: before.8,
            ..Default::default()
        },
    );
    let (width, height) = crate::document_core::DocumentCore::picture_rotated_bounds(
        sa.current_width,
        sa.current_height,
        sa.rotation_angle,
    );
    group.common.width = width;
    group.common.height = height;
    let sa = &mut group.shape_attr;
    sa.rotation_center.x = (width / 2) as i32;
    sa.rotation_center.y = (height / 2) as i32;
    let Some(inverse) = invert(&old_frame) else {
        return true;
    };
    let g = compose(&frame_matrix(&group.common, sa), &inverse);
    let shift = (
        (f64::from(width) - f64::from(before.0)) / 2.0,
        (f64::from(height) - f64::from(before.1)) / 2.0,
    );
    prepend_transform(sa, &g, shift);
    prepend_descendants(&mut group.children, &g, shift);
    true
}

fn prepend_descendants(children: &mut [ShapeObject], g: &[f64; 6], shift: (f64, f64)) {
    for child in children {
        prepend_transform(child.shape_attr_mut(), g, shift);
        if let ShapeObject::Group(group) = child {
            prepend_descendants(&mut group.children, g, shift);
        }
    }
}

/// 합성 행렬 M을 G·M으로 바꾼다. 저장 원본 행렬이 있으면 이동 T와 첫 묶음 단계 회전 R1을
/// T' = T + shift, R1' = T'⁻¹·G·T·R1로 고쳐 같은 합성이 되게 한다(한컴 12.30 저장본과 같은 꼴).
fn prepend_transform(
    sa: &mut crate::model::shape::ShapeComponentAttr,
    g: &[f64; 6],
    shift: (f64, f64),
) {
    let m = compose(
        g,
        &[
            sa.render_sx,
            sa.render_b,
            sa.render_tx,
            sa.render_c,
            sa.render_sy,
            sa.render_ty,
        ],
    );
    [
        sa.render_sx,
        sa.render_b,
        sa.render_tx,
        sa.render_c,
        sa.render_sy,
        sa.render_ty,
    ] = m;
    const MATRIX: usize = 6 * 8;
    if sa.raw_rendering.len() < 2 + 3 * MATRIX
        || u16::from_le_bytes([sa.raw_rendering[0], sa.raw_rendering[1]]) == 0
    {
        sa.raw_rendering = crate::serializer::control::serialize_affine_rendering(sa);
        return;
    }
    let raw = &mut sa.raw_rendering;
    let read = |raw: &[u8], at: usize| {
        let mut m = [0.0; 6];
        for (i, v) in m.iter_mut().enumerate() {
            let o = at + i * 8;
            *v = f64::from_le_bytes(raw[o..o + 8].try_into().expect("8바이트"));
        }
        m
    };
    let (trans, rotation) = (read(raw, 2), read(raw, 2 + 2 * MATRIX));
    let moved = [1.0, 0.0, trans[2] + shift.0, 0.0, 1.0, trans[5] + shift.1];
    let Some(inverse) = invert(&moved) else {
        return;
    };
    let rotated = compose(&compose(&compose(&inverse, g), &trans), &rotation);
    for (at, matrix) in [(2, moved), (2 + 2 * MATRIX, rotated)] {
        for (i, v) in matrix.iter().enumerate() {
            let o = at + i * 8;
            raw[o..o + 8].copy_from_slice(&v.to_le_bytes());
        }
    }
}

/// 자손의 렌더링 행렬 앞에 묶음 로컬 원점 기준 배율을 곱한다. 한컴 12.30은 크기를 current에도
/// 반영하고(버림) 회전 중심을 그 가운데로 옮긴다.
fn scale_descendants(children: &mut [ShapeObject], kx: f64, ky: f64) {
    for child in children {
        let sa = child.shape_attr_mut();
        scale_rendering(sa, kx, ky);
        sa.current_width = (sa.current_width as f64 * kx) as u32;
        sa.current_height = (sa.current_height as f64 * ky) as u32;
        sa.rotation_center.x = (sa.current_width / 2) as i32;
        sa.rotation_center.y = (sa.current_height / 2) as i32;
        if let ShapeObject::Group(group) = child {
            scale_descendants(&mut group.children, kx, ky);
        }
    }
}

/// 합성 행렬 M을 S·M(S = 배율)로 바꾸고, 저장 원본 행렬이 있으면 이동 바로 뒤의 첫 단계
/// (최상위 묶음 단계) 크기 행렬을 같은 합성이 되도록 고친다: T·R0·S0' = S·T·R0·S0.
/// 한컴 12.30 저장본의 자식 첫 단계 크기 행렬 [k, 0, tx(k−1); 0, k, ty(k−1)]과 같은 꼴이다.
fn scale_rendering(sa: &mut crate::model::shape::ShapeComponentAttr, kx: f64, ky: f64) {
    sa.render_sx *= kx;
    sa.render_b *= kx;
    sa.render_tx *= kx;
    sa.render_c *= ky;
    sa.render_sy *= ky;
    sa.render_ty *= ky;
    const MATRIX: usize = 6 * 8;
    let raw = &mut sa.raw_rendering;
    if raw.len() < 2 + 3 * MATRIX || u16::from_le_bytes([raw[0], raw[1]]) == 0 {
        // 원본 행렬이 없으면 저장기가 합성 행렬로 새로 만든다.
        return;
    }
    let read = |raw: &[u8], at: usize| {
        let mut m = [0.0; 6];
        for (i, v) in m.iter_mut().enumerate() {
            let o = at + i * 8;
            *v = f64::from_le_bytes(raw[o..o + 8].try_into().expect("8바이트"));
        }
        m
    };
    let (trans, scale, rotation) = (
        read(raw, 2),
        read(raw, 2 + MATRIX),
        read(raw, 2 + 2 * MATRIX),
    );
    let anchor = compose(&trans, &rotation);
    let Some(inverse) = invert(&anchor) else {
        return;
    };
    let factor = compose(&compose(&inverse, &[kx, 0.0, 0.0, 0.0, ky, 0.0]), &anchor);
    let scaled = compose(&factor, &scale);
    for (i, v) in scaled.iter().enumerate() {
        let o = 2 + MATRIX + i * 8;
        raw[o..o + 8].copy_from_slice(&v.to_le_bytes());
    }
}

/// 아핀 행렬 [a, b, tx, c, d, ty]의 곱 A × B (파서의 합성과 같다).
fn compose(a: &[f64; 6], b: &[f64; 6]) -> [f64; 6] {
    [
        a[0] * b[0] + a[1] * b[3],
        a[0] * b[1] + a[1] * b[4],
        a[0] * b[2] + a[1] * b[5] + a[2],
        a[3] * b[0] + a[4] * b[3],
        a[3] * b[1] + a[4] * b[4],
        a[3] * b[2] + a[4] * b[5] + a[5],
    ]
}

fn invert(m: &[f64; 6]) -> Option<[f64; 6]> {
    let det = m[0] * m[4] - m[1] * m[3];
    if det.abs() < 1e-12 {
        return None;
    }
    Some([
        m[4] / det,
        -m[1] / det,
        (m[1] * m[5] - m[4] * m[2]) / det,
        -m[3] / det,
        m[0] / det,
        (m[3] * m[2] - m[0] * m[5]) / det,
    ])
}
