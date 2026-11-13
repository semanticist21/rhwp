//! 한컴 태그 넣기(셀·셀 블록·표·그림·도형의 HWPX 태그)와 컨테이너 문단 정렬.
//!
//! 한컴 12.30은 셀 태그를 `<hp:subList metatag='{"name":"#이름"}'>`에, 개체 태그를
//! `<hp:metaTag>{"name":"#이름"}</hp:metaTag>`에 쓴다. HWP5에는 자리가 없어 한컴도 HWP로 저장하면 지운다.

use crate::document_core::DocumentCore;
use crate::error::HwpError;
use crate::model::control::Control;
use crate::model::paragraph::Paragraph;

type PathEntry = (usize, usize, usize);

/// 태그 JSON의 이름. 없으면 빈 문자열이다.
fn tag_name(json: Option<&str>) -> String {
    json.and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
        .and_then(|value| value.get("name")?.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// 태그 JSON의 이름만 바꾼다. 다른 키는 남기고, 빈 이름은 태그를 지운다.
fn with_tag_name(json: Option<&str>, name: &str) -> Option<String> {
    if name.is_empty() {
        return None;
    }
    let mut object = json
        .and_then(|json| {
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(json).ok()
        })
        .unwrap_or_default();
    object.insert("name".into(), serde_json::Value::String(name.into()));
    serde_json::to_string(&object).ok()
}

fn control_meta_tag(control: &Control) -> Option<&Option<String>> {
    match control {
        Control::Table(table) => Some(&table.common.meta_tag),
        Control::Picture(picture) => Some(&picture.common.meta_tag),
        Control::Shape(shape) => Some(&shape.common().meta_tag),
        _ => None,
    }
}

fn control_meta_tag_mut(control: &mut Control) -> Option<&mut Option<String>> {
    match control {
        Control::Table(table) => Some(&mut table.common.meta_tag),
        Control::Picture(picture) => Some(&mut picture.common.meta_tag),
        Control::Shape(shape) => Some(&mut shape.common_mut().meta_tag),
        _ => None,
    }
}

fn paragraph_has_meta_tag(paragraph: &Paragraph) -> bool {
    paragraph.controls.iter().any(|control| {
        control_meta_tag(control).is_some_and(Option::is_some)
            || matches!(control, Control::Table(table)
                if table.cells.iter().any(|cell| cell.meta_tag.is_some()))
    })
}

const NOT_TAGGABLE: &str = "태그를 붙일 수 있는 개체가 아닙니다";

impl DocumentCore {
    /// 셀 경로의 마지막 표에서 `cell_idx` 셀의 태그 이름. 없으면 빈 문자열이다.
    pub fn cell_meta_tag_by_path_native(
        &self,
        section_idx: usize,
        parent_para_idx: usize,
        path: &[PathEntry],
        cell_idx: usize,
    ) -> Result<String, HwpError> {
        let table = self.resolve_table_by_path(section_idx, parent_para_idx, path)?;
        let cell = table
            .cells
            .get(cell_idx)
            .ok_or_else(|| HwpError::RenderError(format!("셀 {} 범위 초과", cell_idx)))?;
        Ok(tag_name(cell.meta_tag.as_deref()))
    }

    /// 셀 태그 이름을 바꾼다. 빈 이름은 태그를 지운다.
    pub fn set_cell_meta_tag_by_path_native(
        &mut self,
        section_idx: usize,
        parent_para_idx: usize,
        path: &[PathEntry],
        cell_idx: usize,
        name: &str,
    ) -> Result<(), HwpError> {
        let table = self.resolve_table_mut_by_cell_path(section_idx, parent_para_idx, path)?;
        let cell = table
            .cells
            .get_mut(cell_idx)
            .ok_or_else(|| HwpError::RenderError(format!("셀 {} 범위 초과", cell_idx)))?;
        cell.meta_tag = with_tag_name(cell.meta_tag.as_deref(), name);
        // HWP 원본 스트림에는 태그가 없지만 IR을 고쳤으니 패스스루를 끈다(#2724).
        self.document.sections[section_idx].raw_stream = None;
        Ok(())
    }

    /// 표·그림·도형의 태그 이름. 경로가 비면 본문 문단의 컨트롤이고, 아니면 경로 끝 문단의 컨트롤이다.
    pub fn object_meta_tag_native(
        &self,
        section_idx: usize,
        para_idx: usize,
        path: &[PathEntry],
        control_idx: usize,
    ) -> Result<String, HwpError> {
        let paragraph = if path.is_empty() {
            self.document
                .sections
                .get(section_idx)
                .and_then(|section| section.paragraphs.get(para_idx))
                .ok_or_else(|| HwpError::RenderError(format!("문단 {} 범위 초과", para_idx)))?
        } else {
            self.resolve_paragraph_by_path(section_idx, para_idx, path)?
        };
        let tag = paragraph
            .controls
            .get(control_idx)
            .and_then(control_meta_tag)
            .ok_or_else(|| HwpError::RenderError(NOT_TAGGABLE.into()))?;
        Ok(tag_name(tag.as_deref()))
    }

    /// 표·그림·도형의 태그 이름을 바꾼다. 빈 이름은 태그를 지운다.
    pub fn set_object_meta_tag_native(
        &mut self,
        section_idx: usize,
        para_idx: usize,
        path: &[PathEntry],
        control_idx: usize,
        name: &str,
    ) -> Result<(), HwpError> {
        let paragraph = if path.is_empty() {
            self.document
                .sections
                .get_mut(section_idx)
                .and_then(|section| section.paragraphs.get_mut(para_idx))
                .ok_or_else(|| HwpError::RenderError(format!("문단 {} 범위 초과", para_idx)))?
        } else {
            self.get_cell_paragraph_mut_by_path(section_idx, para_idx, path)?
        };
        let tag = paragraph
            .controls
            .get_mut(control_idx)
            .and_then(control_meta_tag_mut)
            .ok_or_else(|| HwpError::RenderError(NOT_TAGGABLE.into()))?;
        *tag = with_tag_name(tag.as_deref(), name);
        // HWP 원본 스트림에는 태그가 없지만 IR을 고쳤으니 패스스루를 끈다(#2724).
        self.document.sections[section_idx].raw_stream = None;
        Ok(())
    }

    /// 문서 어디든(본문, 표 셀, 글상자, 머리말·꼬리말, 각주·미주) 태그가 있는가.
    /// 한컴은 태그가 있는 문서를 HWP로 저장할 때 태그가 지워진다고 묻는다.
    pub fn has_meta_tags_native(&self) -> bool {
        self.document
            .sections
            .iter()
            .any(|section| section.paragraphs.iter().any(paragraph_has_meta_tag))
            || !self
                .nested_paragraph_paths(&paragraph_has_meta_tag)
                .is_empty()
    }

    /// 셀·글상자·머리말·꼬리말·각주·미주 문단 목록에서 `first`부터 `order.len()`개 문단을 `order`(원래 순번)
    /// 차례로 다시 늘어놓는다. 한컴 정렬처럼 문단을 통째로 옮겨 글자·문단 모양과 개체를 지킨다.
    /// 경로의 마지막 항목이 그 목록을 담은 컨테이너다(문단 번호는 쓰지 않는다).
    pub fn reorder_paragraphs_by_path_native(
        &mut self,
        section_idx: usize,
        parent_para_idx: usize,
        path: &[PathEntry],
        first: usize,
        order: &[usize],
    ) -> Result<(), HwpError> {
        let mut sorted = order.to_vec();
        sorted.sort_unstable();
        if sorted
            .iter()
            .enumerate()
            .any(|(index, &value)| index != value)
        {
            return Err(HwpError::RenderError(
                "정렬 차례가 문단 순번의 순열이 아닙니다".into(),
            ));
        }
        if order
            .iter()
            .enumerate()
            .all(|(index, &value)| index == value)
        {
            return Ok(());
        }
        let paragraphs =
            self.get_cell_paragraphs_mut_by_path(section_idx, parent_para_idx, path)?;
        let end = first + order.len();
        if end > paragraphs.len() {
            return Err(HwpError::RenderError(format!(
                "문단 {}..{} 범위 초과 (총 {}개)",
                first,
                end,
                paragraphs.len()
            )));
        }
        let mut moved: Vec<Option<Paragraph>> = paragraphs.drain(first..end).map(Some).collect();
        let reordered: Vec<Paragraph> = order
            .iter()
            .map(|&index| moved[index].take().expect("순열이라 한 번씩만 꺼낸다"))
            .collect();
        paragraphs.splice(first..first, reordered);

        self.recalculate_cell_paragraph_vpos_by_path(
            section_idx,
            parent_para_idx,
            path,
            first,
            None,
        );
        self.mark_cell_control_dirty(section_idx, parent_para_idx, path[0].0);
        self.document.sections[section_idx].raw_stream = None;
        self.mark_section_dirty(section_idx);
        self.paginate_if_needed();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_hancom_picture_and_shape_tags() {
        // 한컴 12.30이 사각형 태그를 쓴 꼴(설명 뒤 마지막 자식).
        let xml = concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            r#"<hs:sec xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph""#,
            r#" xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section">"#,
            r#"<hp:p id="0" paraPrIDRef="0" styleIDRef="0"><hp:run charPrIDRef="0">"#,
            r#"<hp:rect id="1" zOrder="0" numberingType="NONE" textWrap="TOP_AND_BOTTOM" textFlow="BOTH_SIDES" lock="0" dropcapstyle="None" instid="1" ratio="0">"#,
            r#"<hp:sz width="8000" widthRelTo="ABSOLUTE" height="3000" heightRelTo="ABSOLUTE" protect="0"/>"#,
            r#"<hp:pos treatAsChar="1" affectLSpacing="0" flowWithText="0" allowOverlap="0" holdAnchorAndSO="0" vertRelTo="PARA" horzRelTo="PARA" vertAlign="TOP" horzAlign="LEFT" vertOffset="0" horzOffset="0"/>"#,
            r#"<hp:outMargin left="0" right="0" top="0" bottom="0"/>"#,
            r##"<hp:shapeComment>사각형입니다.</hp:shapeComment><hp:metaTag>{"name":"#rectR"}</hp:metaTag></hp:rect>"##,
            r#"<hp:t/></hp:run></hp:p></hs:sec>"#
        );
        let section = crate::parser::hwpx::section::parse_hwpx_section(xml).unwrap();
        let Control::Shape(shape) = &section.paragraphs[0].controls[0] else {
            panic!("사각형이어야 한다");
        };
        assert_eq!(shape.common().description, "사각형입니다.");
        assert_eq!(tag_name(shape.common().meta_tag.as_deref()), "#rectR");
    }

    #[test]
    fn tag_name_keeps_other_keys() {
        let json = with_tag_name(Some(r##"{"name":"#a","other":1}"##), "#b").unwrap();
        assert_eq!(tag_name(Some(&json)), "#b");
        assert!(json.contains("\"other\":1"));
        assert_eq!(with_tag_name(Some(&json), ""), None);
    }
}
