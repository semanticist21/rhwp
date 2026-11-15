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

/// 회전·대칭 없는 도형의 원본 점 → 틀 좌표 변환 (a, b, tx, c, d, ty).
fn frame_affine(shape: &ShapeObject) -> Result<[f64; 6], HwpError> {
    let sa = shape.shape_attr();
    if sa.rotation_angle != 0 || sa.horz_flip || sa.vert_flip {
        return Err(invalid(
            "회전하거나 뒤집은 도형의 꼭짓점은 아직 고칠 수 없습니다",
        ));
    }
    // 렌더러처럼 다각형의 저장 행렬은 점의 최종 변환이다.
    if let ShapeObject::Polygon(poly) = shape {
        if sa.has_saved_affine_geometry() || poly.affine_edit_reference.is_some() {
            return Ok([
                sa.render_sx,
                sa.render_b,
                sa.render_tx,
                sa.render_c,
                sa.render_sy,
                sa.render_ty,
            ]);
        }
    }
    let c = shape.common();
    let scale = |frame: u32, original: u32| {
        if original > 0 {
            f64::from(frame) / f64::from(original)
        } else {
            1.0
        }
    };
    Ok([
        scale(c.width, sa.original_width),
        0.0,
        0.0,
        0.0,
        scale(c.height, sa.original_height),
        0.0,
    ])
}

fn points_of(shape: &ShapeObject) -> Result<(&[Point], Option<&[u8]>), HwpError> {
    match shape {
        ShapeObject::Polygon(poly) => Ok((&poly.points, None)),
        ShapeObject::Curve(curve) => Ok((&curve.points, Some(&curve.segment_types))),
        _ => Err(invalid("다각형이나 곡선이 아닙니다")),
    }
}

impl DocumentCore {
    /// 본문 다각형·곡선의 꼭짓점(틀 기준 HWPUNIT)과 곡선의 구간 종류(0 직선, 1 곡선).
    ///
    /// 반환: `{"kind":"polygon"|"curve","points":[[x,y],...],"segmentTypes":[...]}`
    pub fn get_shape_points_native(
        &self,
        section_idx: usize,
        parent_para_idx: usize,
        control_idx: usize,
    ) -> Result<String, HwpError> {
        let shape = self.resolve_shape_control_ref(section_idx, parent_para_idx, control_idx)?;
        let (points, segments) = points_of(shape)?;
        let [a, b, tx, c, d, ty] = frame_affine(shape)?;
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
            "kind": if segments.is_some() { "curve" } else { "polygon" },
            "points": points,
            "segmentTypes": segments.unwrap_or(&[]),
        })
        .to_string())
    }

    /// 본문 다각형·곡선의 꼭짓점을 바꾼다(한컴 다각형 편집·열린 다각형 닫기/닫힌 다각형 열기).
    ///
    /// json: `{"points":[[x,y],...],"segmentTypes":[...]}` — 틀 기준 HWPUNIT. 곡선의 구간 종류를
    /// 빼면 점 수가 같을 때 그대로, 다르면 모두 곡선으로 둔다. 점 상자가 틀을 벗어나거나 줄면
    /// 틀의 위치·크기를 점 상자에 맞춘다.
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

        let band_before = {
            let shape =
                self.resolve_shape_control_ref(section_idx, parent_para_idx, control_idx)?;
            points_of(shape)?;
            frame_affine(shape)?;
            let c = shape.common();
            // 틀을 옮기는 기준이 왼쪽·위일 때만 위치 이동으로 같은 자리에 둔다.
            if !c.treat_as_char
                && (c.horz_align != HorzAlign::Left || c.vert_align != VertAlign::Top)
            {
                return Err(invalid(
                    "왼쪽·위 기준이 아닌 도형의 꼭짓점은 아직 고칠 수 없습니다",
                ));
            }
            super::super::picture::picture_band_geometry(c)
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
            c.horizontal_offset = c.horizontal_offset.wrapping_add_signed(min_x);
            c.vertical_offset = c.vertical_offset.wrapping_add_signed(min_y);
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
            // 점이 틀 좌표 그대로이므로 저장 행렬을 남기지 않는다.
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
