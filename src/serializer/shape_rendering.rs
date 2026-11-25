//! HWP/HWPX가 공유하는 도형 저장 변환. 원본 좌표 → 크기 → 회전 → 이동 순서다.
//! 한컴 12.30 저장본은 좌우·상하 대칭을 (그 이동과 함께) 회전 행렬에 두고, 크기 행렬에는
//! 양수 배율만 둔다(가로 대칭 4500×3000 그림: 이동·크기 = 단위, 회전 = [-1 0 4500; 0 1 0]).

use crate::model::shape::ShapeComponentAttr;

pub(super) type Matrix = [f64; 6];
pub(super) type RenderMatrices = (Matrix, Vec<(Matrix, Matrix)>);
const IDENTITY: Matrix = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];

/// 원본 행렬이 없는 편집 개체의 변환을 만든다. 기존 HWP 저장 계약을 양 형식에 적용한다.
pub(super) fn generated(attr: &ShapeComponentAttr) -> RenderMatrices {
    let (trans, scale, rotation) = if attr.rotation_angle.rem_euclid(360) != 0 {
        (
            [
                1.0,
                0.0,
                attr.offset_x as f64,
                0.0,
                1.0,
                attr.offset_y as f64,
            ],
            shape_scale_matrix(attr),
            compose(
                &shape_rotation_matrix(attr),
                &scaled_reflection_matrix(attr, &shape_scale_matrix(attr)),
            ),
        )
    } else if has_explicit_rendering_matrix(attr) {
        let (sx, sy) = (attr.render_sx, attr.render_sy);
        if attr.render_b == 0.0 && attr.render_c == 0.0 && (sx < 0.0 || sy < 0.0) {
            // 대칭은 회전 칸에 둔다. 크기 칸의 음수 배율은 한컴이 위아래로 한 번 더 뒤집어 그린다.
            let (ox, oy) = (attr.offset_x as f64, attr.offset_y as f64);
            (
                [1.0, 0.0, ox, 0.0, 1.0, oy],
                [sx.abs(), 0.0, 0.0, 0.0, sy.abs(), 0.0],
                [
                    sx.signum(),
                    0.0,
                    attr.render_tx - ox,
                    0.0,
                    sy.signum(),
                    attr.render_ty - oy,
                ],
            )
        } else {
            (
                [1.0, 0.0, attr.render_tx, 0.0, 1.0, attr.render_ty],
                [sx, attr.render_b, 0.0, attr.render_c, sy, 0.0],
                IDENTITY,
            )
        }
    } else {
        // 위치의 권위는 명시적 render_tx/ty다. offset을 임의로 이동 행렬로 승격하지 않는다.
        // 크기는 current/original로 표현하므로 회전 없는 폴백에 배율을 중복 기록하지 않는다.
        (IDENTITY, IDENTITY, attr.local_reflection_matrix())
    };
    let mut pairs = vec![(scale, rotation)];
    // 한컴 HWP5 저장본의 행렬 쌍 개수는 그룹 깊이 + 1이다.
    pairs.extend((1..attr.group_level.saturating_add(1)).map(|_| (IDENTITY, IDENTITY)));
    (trans, pairs)
}

fn has_explicit_rendering_matrix(attr: &ShapeComponentAttr) -> bool {
    const EPSILON: f64 = 0.000_001;

    (attr.render_sx - 1.0).abs() > EPSILON
        || attr.render_b.abs() > EPSILON
        || attr.render_tx.abs() > EPSILON
        || attr.render_c.abs() > EPSILON
        || (attr.render_sy - 1.0).abs() > EPSILON
        || attr.render_ty.abs() > EPSILON
}

fn shape_scale_matrix(attr: &ShapeComponentAttr) -> [f64; 6] {
    let sx = if attr.original_width > 0 && attr.current_width > 0 {
        attr.current_width as f64 / attr.original_width as f64
    } else {
        1.0
    };
    let sy = if attr.original_height > 0 && attr.current_height > 0 {
        attr.current_height as f64 / attr.original_height as f64
    } else {
        1.0
    };
    [sx, 0.0, 0.0, 0.0, sy, 0.0]
}

/// 크기를 바꾼 상자 안의 대칭. 크기 행렬 뒤(회전 칸)에 두어도 원본 상자 대칭을 크기보다 먼저
/// 적용한 것과 같은 결과를 낸다.
fn scaled_reflection_matrix(attr: &ShapeComponentAttr, scale: &Matrix) -> Matrix {
    let mut reflection = attr.local_reflection_matrix();
    reflection[2] *= scale[0];
    reflection[5] *= scale[4];
    reflection
}

/// 두 아핀 행렬 합성 `a × b`(b를 먼저 적용한다).
fn compose(a: &Matrix, b: &Matrix) -> Matrix {
    [
        a[0] * b[0] + a[1] * b[3],
        a[0] * b[1] + a[1] * b[4],
        a[0] * b[2] + a[1] * b[5] + a[2],
        a[3] * b[0] + a[4] * b[3],
        a[3] * b[1] + a[4] * b[4],
        a[3] * b[2] + a[4] * b[5] + a[5],
    ]
}

fn shape_rotation_positive_correction(attr: &ShapeComponentAttr) -> (f64, f64) {
    let width = attr.current_width as f64;
    let height = attr.current_height as f64;
    if width <= 0.0 || height <= 0.0 {
        return (0.0, 0.0);
    }

    let theta = (attr.rotation_angle as f64).to_radians();
    let cos = theta.cos();
    let sin = theta.sin();
    let corners = [
        (0.0, 0.0),
        (width * cos, width * sin),
        (-height * sin, height * cos),
        (width * cos - height * sin, width * sin + height * cos),
    ];
    let min_x = corners
        .iter()
        .map(|(x, _)| *x)
        .fold(f64::INFINITY, f64::min);
    let min_y = corners
        .iter()
        .map(|(_, y)| *y)
        .fold(f64::INFINITY, f64::min);
    (-min_x, -min_y)
}

fn shape_rotation_matrix(attr: &ShapeComponentAttr) -> [f64; 6] {
    if attr.rotation_angle.rem_euclid(360) == 0 {
        return [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    }

    let theta = (attr.rotation_angle as f64).to_radians();
    let cos = theta.cos();
    let sin = theta.sin();
    let (tx, ty) = shape_rotation_positive_correction(attr);

    [
        cos,
        -sin,
        tx - attr.offset_x as f64,
        sin,
        cos,
        ty - attr.offset_y as f64,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 파서가 다시 열 때 쓰는 합성 `a × b`(b를 먼저 적용한다).
    fn mul(a: &Matrix, b: &Matrix) -> Matrix {
        [
            a[0] * b[0] + a[1] * b[3],
            a[0] * b[1] + a[1] * b[4],
            a[0] * b[2] + a[1] * b[5] + a[2],
            a[3] * b[0] + a[4] * b[3],
            a[3] * b[1] + a[4] * b[4],
            a[3] * b[2] + a[4] * b[5] + a[5],
        ]
    }

    fn picture(horz: bool, vert: bool, angle: i16) -> ShapeComponentAttr {
        let mut attr = ShapeComponentAttr {
            original_width: 4500,
            original_height: 3000,
            current_width: 4500,
            current_height: 3000,
            horz_flip: horz,
            vert_flip: vert,
            rotation_angle: angle,
            ..Default::default()
        };
        // 그림 속성 대칭이 남기는 명시적 행렬(object_ops/picture.rs).
        let [sx, b, tx, c, sy, ty] = attr.local_reflection_matrix();
        (attr.render_sx, attr.render_b, attr.render_tx) = (sx, b, tx);
        (attr.render_c, attr.render_sy, attr.render_ty) = (c, sy, ty);
        attr
    }

    fn composite((trans, pairs): &RenderMatrices) -> Matrix {
        pairs.iter().fold(*trans, |acc, (scale, rotation)| {
            mul(&mul(&acc, rotation), scale)
        })
    }

    /// 한컴 12.30 저장본(`hc-exif.hwp`)처럼 대칭은 이동과 함께 회전 칸에 들고 이동·크기는 단위다.
    #[test]
    fn picture_flip_is_saved_in_the_rotation_slot_like_hancom() {
        let (trans, pairs) = generated(&picture(true, false, 0));
        assert_eq!(trans, IDENTITY);
        assert_eq!(pairs, vec![(IDENTITY, [-1.0, 0.0, 4500.0, 0.0, 1.0, 0.0])]);
        let (trans, pairs) = generated(&picture(false, true, 0));
        assert_eq!(trans, IDENTITY);
        assert_eq!(pairs, vec![(IDENTITY, [1.0, 0.0, 0.0, 0.0, -1.0, 3000.0])]);
    }

    /// 회전한 그림도 크기 칸에는 양수 배율만 두고 대칭은 회전과 합성한다. 다시 열 때 합성하는
    /// 변환(파서의 이동 × 회전 × 크기)은 종전과 같아야 우리 렌더가 그대로다.
    #[test]
    fn rotated_picture_flip_composes_into_the_rotation_slot() {
        for angle in [90, 270] {
            let mut attr = picture(true, false, angle);
            attr.current_width = 9000;
            let matrices = generated(&attr);
            let (scale, rotation) = matrices.1[0];
            assert!(scale[0] > 0.0 && scale[4] > 0.0, "{angle}: {scale:?}");
            let det = rotation[0] * rotation[4] - rotation[1] * rotation[3];
            assert!((det + 1.0).abs() < 1e-9, "{angle}: {rotation:?}");
            let before = mul(
                &mul(&matrices.0, &shape_rotation_matrix(&attr)),
                &mul(&shape_scale_matrix(&attr), &attr.local_reflection_matrix()),
            );
            let after = composite(&matrices);
            assert!(
                before.iter().zip(after).all(|(a, b)| (a - b).abs() < 1e-6),
                "{angle}: {before:?} {after:?}"
            );
        }
    }
}
