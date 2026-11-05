//! 메모 모양 읽기·바꾸기(한컴 입력 > 메모 > 메모 모양).
//!
//! 한컴 12.30 Mac은 문서 전체에 적용하면 구역이 가리키는 모양을 고쳐 쓰고, 새 구역으로 적용하면
//! 새 모양을 더해 새 구역만 가리키게 한다. 형식은 `model::memo_shape`에 있다.

use super::memo_query::{MemoError, MemoErrorCode};
use crate::document_core::DocumentCore;
use crate::model::control::Control;
use crate::model::memo_shape::{
    color_from_hex, color_hex, memo_shapes, set_memo_shapes, MemoShape,
};

fn invalid(message: &str) -> MemoError {
    MemoError::new(MemoErrorCode::InvalidRange, message)
}

impl DocumentCore {
    /// 구역이 쓰는 메모 모양. 모양을 가리키지 않거나 목록에 없으면 한컴 기본값이다.
    pub fn memo_shape(&self, section_idx: usize) -> Option<MemoShape> {
        let section = self.document.sections.get(section_idx)?;
        let id = usize::from(section.section_def.memo_shape_id);
        Some(
            id.checked_sub(1)
                .and_then(|index| memo_shapes(&self.document.doc_info).get(index).copied())
                .unwrap_or_default(),
        )
    }

    /// `getMemoShape`: `{width,lineType,lineWidth,lineColor,fillColor,activeColor}`.
    /// 너비는 HWPUNIT, 색은 `#RRGGBB`, 선 굵기는 한컴 메모 모양 창의 굵기 목록 순번이다.
    pub fn get_memo_shape_json(&self, section_idx: usize) -> Result<String, MemoError> {
        let shape = self
            .memo_shape(section_idx)
            .ok_or_else(|| invalid("구역 위치를 찾을 수 없음"))?;
        Ok(format!(
            r#"{{"width":{},"lineType":{},"lineWidth":{},"lineColor":"{}","fillColor":"{}","activeColor":"{}"}}"#,
            shape.width,
            shape.line_type,
            shape.line_width,
            color_hex(shape.line_color),
            color_hex(shape.fill_color),
            color_hex(shape.active_color),
        ))
    }

    /// `setMemoShape`: 메모 모양을 한 구역(`section`) 또는 모든 구역(None)에 준다. 대상 구역이 모두
    /// 같은 모양을 가리키고 다른 구역이 그 모양을 쓰지 않으면 그 모양을 고쳐 쓰고, 아니면 새 모양을
    /// 더해 대상 구역이 가리키게 한다.
    pub fn set_memo_shape_json(
        &mut self,
        section: Option<usize>,
        json: &str,
    ) -> Result<String, MemoError> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|_| invalid("메모 모양은 JSON 객체여야 함"))?;
        let number = |key: &str, max: u64| {
            value
                .get(key)
                .and_then(serde_json::Value::as_u64)
                .filter(|n| *n <= max)
                .ok_or_else(|| invalid(&format!("메모 모양 {key} 값이 맞지 않음")))
        };
        let color = |key: &str| {
            value
                .get(key)
                .and_then(serde_json::Value::as_str)
                .and_then(color_from_hex)
                .ok_or_else(|| invalid(&format!("메모 모양 {key}는 #RRGGBB여야 함")))
        };
        let width = number("width", u64::from(i32::MAX as u32))?;
        if width == 0 {
            return Err(invalid("메모 모양 width 값이 맞지 않음"));
        }
        let previous = self
            .memo_shape(section.unwrap_or(0))
            .ok_or_else(|| invalid("구역 위치를 찾을 수 없음"))?;
        let shape = MemoShape {
            width: width as u32,
            line_type: number("lineType", 13)? as u8,
            line_width: number("lineWidth", 15)? as u8,
            line_color: color("lineColor")?,
            fill_color: color("fillColor")?,
            active_color: color("activeColor")?,
            memo_type: previous.memo_type,
        };

        let targets: Vec<usize> = match section {
            Some(index) => vec![index],
            None => (0..self.document.sections.len()).collect(),
        };
        let mut shapes = memo_shapes(&self.document.doc_info);
        let refs: Vec<u16> = self
            .document
            .sections
            .iter()
            .map(|section| section.section_def.memo_shape_id)
            .collect();
        let target_ref = refs[targets[0]];
        let shared = targets.iter().all(|&index| refs[index] == target_ref)
            && refs
                .iter()
                .enumerate()
                .all(|(index, id)| *id != target_ref || targets.contains(&index));
        let id = if target_ref != 0 && usize::from(target_ref) <= shapes.len() && shared {
            shapes[usize::from(target_ref) - 1] = shape;
            target_ref
        } else {
            shapes.push(shape);
            u16::try_from(shapes.len()).map_err(|_| invalid("메모 모양이 너무 많음"))?
        };
        set_memo_shapes(&mut self.document.doc_info, &shapes);
        for index in targets {
            let section = &mut self.document.sections[index];
            section.section_def.memo_shape_id = id;
            for ctrl in section
                .paragraphs
                .first_mut()
                .map(|para| para.controls.iter_mut())
                .into_iter()
                .flatten()
            {
                if let Control::SectionDef(def) = ctrl {
                    def.memo_shape_id = id;
                }
            }
            section.raw_stream = None;
            self.mark_section_dirty(index);
        }
        Ok(r#"{"ok":true}"#.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core() -> DocumentCore {
        let mut core = DocumentCore::new_empty();
        core.create_blank_document_native().unwrap();
        core
    }

    const SHAPE: &str = r##"{"width":19842,"lineType":2,"lineWidth":7,"lineColor":"#C75252","fillColor":"#DFEAF5","activeColor":"#FAEDD2"}"##;

    #[test]
    fn document_wide_shape_round_trips_in_hwp_and_hwpx() {
        let mut core = core();
        assert!(core
            .get_memo_shape_json(0)
            .unwrap()
            .contains(r##""lineColor":"#B6D7AE""##));
        core.set_memo_shape_json(None, SHAPE).unwrap();
        assert_eq!(core.get_memo_shape_json(0).unwrap(), SHAPE);
        // 문서 전체에 다시 주면 같은 모양을 고쳐 쓴다.
        core.set_memo_shape_json(None, &SHAPE.replace("19842", "20000"))
            .unwrap();
        assert_eq!(memo_shapes(&core.document.doc_info).len(), 1);

        for bytes in [
            core.export_hwp_native().unwrap(),
            core.export_hwpx_native().unwrap(),
        ] {
            let reopened = DocumentCore::from_bytes(&bytes).unwrap();
            assert_eq!(
                reopened.get_memo_shape_json(0).unwrap(),
                SHAPE.replace("19842", "20000")
            );
        }
    }

    #[test]
    fn hancom_new_section_shapes_follow_each_section() {
        // 한컴 12.30 Mac: 삼중선 3.0mm 모양을 문서 전체에 준 뒤 0.12mm를 새 구역으로 적용한 문서.
        // 앞 구역은 앞 모양을, 새 구역은 새 모양을 가리킨다.
        let check = |core: &DocumentCore| {
            assert!(core
                .get_memo_shape_json(0)
                .unwrap()
                .contains(r#""lineType":11,"lineWidth":12"#));
            assert!(core
                .get_memo_shape_json(1)
                .unwrap()
                .contains(r#""lineType":11,"lineWidth":1,"#));
        };
        for bytes in [
            include_bytes!("../../../tests/fixtures/memo_hancom/shape-new-section.hwp").as_slice(),
            include_bytes!("../../../tests/fixtures/memo_hancom/shape-new-section.hwpx").as_slice(),
        ] {
            let core = DocumentCore::from_bytes(bytes).unwrap();
            check(&core);
            for out in [
                core.export_hwp_native().unwrap(),
                core.export_hwpx_native().unwrap(),
            ] {
                check(&DocumentCore::from_bytes(&out).unwrap());
            }
        }
    }

    #[test]
    fn rejects_a_shape_it_cannot_store() {
        let mut core = core();
        assert!(core
            .set_memo_shape_json(None, &SHAPE.replace("#C75252", "red"))
            .is_err());
        assert!(core
            .set_memo_shape_json(None, &SHAPE.replace(r#""lineType":2"#, r#""lineType":20"#))
            .is_err());
        assert!(core.set_memo_shape_json(Some(9), SHAPE).is_err());
    }
}
