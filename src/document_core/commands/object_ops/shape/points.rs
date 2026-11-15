//! 한컴 다각형 편집: 다각형·곡선 꼭짓점 읽기·고치기.
//!
//! 점은 개체 틀 기준 HWPUNIT(틀 왼쪽 위가 0)이다. 고친 뒤에는 점 상자에 맞춰 틀을 다시 잡고
//! (위치·크기를 옮기고) 원본 크기를 틀 크기로 두어 배율·저장 행렬을 남기지 않는다.

use crate::document_core::DocumentCore;
use crate::error::HwpError;
use crate::model::shape::{HorzAlign, ShapeObject, VertAlign};
use crate::model::Point;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PointsRequest {
    points: Vec<[i32; 2]>,
    #[serde(default)]
    segment_types: Option<Vec<u8>>,
}

fn invalid(message: &str) -> HwpError {
    HwpError::RenderError(message.to_string())
}

/// 원본 점 → 틀 좌표(틀 왼쪽 위 0, HWPUNIT) 아핀 변환 (a, b, tx, c, d, ty). 렌더러가 그리는 자리와 같다.
fn frame_affine(shape: &ShapeObject) -> [f64; 6] {
    let sa = shape.shape_attr();
    // 렌더러처럼 다각형의 저장 행렬은 점의 최종 변환이다(회전·대칭이 이미 들어 있다).
    if let ShapeObject::Polygon(poly) = shape {
        if sa.has_saved_affine_geometry() || poly.affine_edit_reference.is_some() {
            return [
                sa.render_sx,
                sa.render_b,
                sa.render_tx,
                sa.render_c,
                sa.render_sy,
                sa.render_ty,
            ];
        }
    }
    let c = shape.common();
    let (w, h) = (f64::from(c.width), f64::from(c.height));
    // 돌리거나 뒤집은 도형은 현재 크기 상자를 틀 가운데 두고, 대칭한 뒤 가운데를 축으로 돌린다.
    let turned = sa.rotation_angle != 0 || sa.horz_flip || sa.vert_flip;
    let (bw, bh) = if turned && sa.current_width > 0 && sa.current_height > 0 {
        (f64::from(sa.current_width), f64::from(sa.current_height))
    } else {
        (w, h)
    };
    let scale = |size: f64, original: u32| {
        if original > 0 {
            size / f64::from(original)
        } else {
            1.0
        }
    };
    let (sx, sy) = (scale(bw, sa.original_width), scale(bh, sa.original_height));
    let (sin, cos) = f64::from(sa.rotation_angle).to_radians().sin_cos();
    let fx = if sa.horz_flip { -1.0 } else { 1.0 };
    let fy = if sa.vert_flip { -1.0 } else { 1.0 };
    [
        cos * fx * sx,
        -sin * fy * sy,
        w / 2.0 - cos * fx * bw / 2.0 + sin * fy * bh / 2.0,
        sin * fx * sx,
        cos * fy * sy,
        h / 2.0 - sin * fx * bw / 2.0 - cos * fy * bh / 2.0,
    ]
}

/// 도형 종류와 점·구간 종류. 타원은 점 없이 변환만 준다.
fn points_of(shape: &ShapeObject) -> Result<(&'static str, &[Point], Option<&[u8]>), HwpError> {
    match shape {
        ShapeObject::Polygon(poly) => Ok(("polygon", &poly.points, None)),
        ShapeObject::Curve(curve) => Ok(("curve", &curve.points, Some(&curve.segment_types))),
        ShapeObject::Ellipse(_) => Ok(("ellipse", &[], None)),
        _ => Err(invalid("다각형·곡선·타원이 아닙니다")),
    }
}

impl DocumentCore {
    /// 본문 다각형·곡선의 꼭짓점(틀 기준 HWPUNIT, 돌리거나 뒤집은 뒤 보이는 자리)과 곡선의
    /// 구간 종류(0 직선, 1 곡선). 타원은 점 없이 같은 변환만 준다.
    ///
    /// 반환: `{"kind":"polygon"|"curve"|"ellipse","points":[[x,y],...],"segmentTypes":[...],
    /// "affine":[a,b,tx,c,d,ty],"original":[w,h],"axesMirrored":bool}` — affine은 원본 좌표 → 틀 좌표다.
    pub fn get_shape_points_native(
        &self,
        section_idx: usize,
        parent_para_idx: usize,
        control_idx: usize,
    ) -> Result<String, HwpError> {
        let shape = self.resolve_shape_control_ref(section_idx, parent_para_idx, control_idx)?;
        let (kind, points, segments) = points_of(shape)?;
        let affine = frame_affine(shape);
        let [a, b, tx, c, d, ty] = affine;
        let points: Vec<[i64; 2]> = points
            .iter()
            .map(|p| {
                let (x, y) = (f64::from(p.x), f64::from(p.y));
                [
                    (a * x + b * y + tx).round() as i64,
                    (c * x + d * y + ty).round() as i64,
                ]
            })
            .collect();
        Ok(serde_json::json!({
            "kind": kind,
            "points": points,
            "segmentTypes": segments.unwrap_or(&[]),
            "affine": affine,
            "original": [shape.shape_attr().original_width, shape.shape_attr().original_height],
            // 타원의 제2축이 아래(rhwp가 만든 타원)인지. 한컴은 제1축→제2축 쪽으로 호를 긋는다.
            "axesMirrored": matches!(shape, ShapeObject::Ellipse(e) if e.arc_axes_mirrored()),
        })
        .to_string())
    }

    /// 본문 다각형·곡선의 꼭짓점을 바꾼다(한컴 다각형 편집·열린 다각형 닫기/닫힌 다각형 열기).
    ///
    /// json: `{"points":[[x,y],...],"segmentTypes":[...]}` — 틀 기준 HWPUNIT. 곡선의 구간 종류를
    /// 빼면 점 수가 같을 때 그대로, 다르면 모두 곡선으로 둔다. 점 상자가 틀을 벗어나거나 줄면
    /// 틀의 위치·크기를 점 상자에 맞춘다. 한컴 12.30처럼 돌리거나 뒤집은 도형은 보이는 자리 그대로
    /// 점에 새기고 회전각·대칭을 지운다. 가운데·오른쪽(아래) 기준 도형은 기준을 두고 위치값을 고쳐
    /// 다른 점을 제자리에 둔다.
    pub fn set_shape_points_native(
        &mut self,
        section_idx: usize,
        parent_para_idx: usize,
        control_idx: usize,
        json: &str,
    ) -> Result<String, HwpError> {
        let request: PointsRequest = serde_json::from_str(json)
            .map_err(|e| invalid(&format!("꼭짓점 JSON을 읽지 못했습니다: {e}")))?;
        let count = request.points.len();
        if count < 2 {
            return Err(invalid("꼭짓점이 두 개 이상 있어야 합니다"));
        }
        let min_x = request.points.iter().map(|p| p[0]).min().unwrap_or(0);
        let min_y = request.points.iter().map(|p| p[1]).min().unwrap_or(0);
        let max_x = request.points.iter().map(|p| p[0]).max().unwrap_or(0);
        let max_y = request.points.iter().map(|p| p[1]).max().unwrap_or(0);
        let width = u32::try_from(i64::from(max_x) - i64::from(min_x))
            .map_err(|_| invalid("꼭짓점 범위가 너무 큽니다"))?
            .max(1);
        let height = u32::try_from(i64::from(max_y) - i64::from(min_y))
            .map_err(|_| invalid("꼭짓점 범위가 너무 큽니다"))?
            .max(1);
        let points: Vec<Point> = request
            .points
            .iter()
            .map(|p| Point {
                x: p[0] - min_x,
                y: p[1] - min_y,
            })
            .collect();

        let (band_before, old_width, old_height) = {
            let shape =
                self.resolve_shape_control_ref(section_idx, parent_para_idx, control_idx)?;
            if points_of(shape)?.0 == "ellipse" {
                return Err(invalid("다각형이나 곡선이 아닙니다"));
            }
            let c = shape.common();
            (
                super::super::picture::picture_band_geometry(c),
                i64::from(c.width),
                i64::from(c.height),
            )
        };
        let shape = self.resolve_shape_control_mut(section_idx, parent_para_idx, control_idx)?;
        match shape {
            ShapeObject::Polygon(poly) => {
                poly.points = points;
                poly.affine_edit_reference = None;
            }
            ShapeObject::Curve(curve) => {
                curve.segment_types = match request.segment_types {
                    Some(types) if types.len() == count - 1 => {
                        types.into_iter().map(|t| t.min(1)).collect()
                    }
                    Some(_) => {
                        return Err(invalid("구간 종류 수가 꼭짓점 수보다 하나 적어야 합니다"))
                    }
                    None if curve.segment_types.len() == count - 1 => {
                        std::mem::take(&mut curve.segment_types)
                    }
                    None => vec![1; count - 1],
                };
                curve.points = points;
            }
            _ => unreachable!("points_of가 다각형·곡선만 받는다"),
        }
        let c = shape.common_mut();
        if !c.treat_as_char {
            // 틀 왼쪽(위)이 min만큼 옮겨 가도록 기준별 위치값을 고친다(렌더러 aligned_horizontal_origin).
            let (dw, dh) = (i64::from(width) - old_width, i64::from(height) - old_height);
            let (min_x, min_y) = (i64::from(min_x), i64::from(min_y));
            let dx = match c.horz_align {
                HorzAlign::Left | HorzAlign::Inside => min_x,
                HorzAlign::Center => min_x + dw.div_euclid(2),
                HorzAlign::Right | HorzAlign::Outside => -(min_x + dw),
            };
            let dy = match c.vert_align {
                VertAlign::Top | VertAlign::Inside => min_y,
                VertAlign::Center => min_y + dh.div_euclid(2),
                VertAlign::Bottom | VertAlign::Outside => -(min_y + dh),
            };
            c.horizontal_offset = (i64::from(c.horizontal_offset as i32) + dx) as i32 as u32;
            c.vertical_offset = (i64::from(c.vertical_offset as i32) + dy) as i32 as u32;
        }
        c.width = width;
        c.height = height;
        if let Some(d) = shape.drawing_mut() {
            let sa = &mut d.shape_attr;
            sa.original_width = width;
            sa.original_height = height;
            sa.current_width = width;
            sa.current_height = height;
            sa.rotation_center = Point {
                x: (width / 2) as i32,
                y: (height / 2) as i32,
            };
            // 점이 보이는 자리 그대로이므로 회전·대칭·저장 행렬을 남기지 않는다(한컴 12.30 저장본).
            sa.rotation_angle = 0;
            sa.horz_flip = false;
            sa.vert_flip = false;
            sa.flip &= !3;
            sa.raw_rendering.clear();
            sa.render_sx = 1.0;
            sa.render_sy = 1.0;
            sa.render_b = 0.0;
            sa.render_c = 0.0;
            sa.render_tx = 0.0;
            sa.render_ty = 0.0;
        }
        // set_shape_properties_native와 같은 뒤처리: 다시 짜고, 어울림 밴드가 바뀌면 저장 줄을 새긴다.
        self.document.sections[section_idx].raw_stream = None;
        self.recompose_section(section_idx);
        self.paginate_if_needed();
        self.invalidate_page_tree_cache();
        let band_after = self
            .resolve_shape_control_ref(section_idx, parent_para_idx, control_idx)
            .ok()
            .map(|shape| super::super::picture::picture_band_geometry(shape.common()));
        if band_after.as_ref() != Some(&band_before) {
            let _ = self.apply_body_edit_through_picture_band(section_idx, parent_para_idx, |_| {});
        }
        self.event_log
            .push(crate::model::event::DocumentEvent::PictureResized {
                section: section_idx,
                para: parent_para_idx,
                ctrl: control_idx,
            });
        Ok("{\"ok\":true}".to_string())
    }
}
