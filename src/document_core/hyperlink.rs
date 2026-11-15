//! 본문·표 셀·글상자 단일 문단의 텍스트 하이퍼링크 편집 계약 (#6963).
//! offset은 UTF-16/바이트가 아닌 Unicode scalar 인덱스이며 범위는 [start, end)다.
//! 개체를 감싼 필드·다단락 필드·캡션은 이 API의 대상이 아니다.

use super::DocumentCore;
use crate::error::HwpError;
use crate::model::control::{Control, Field, FieldType, Parameter};
use crate::model::event::DocumentEvent;
use crate::model::hyperlink::{
    bookmark_command, classify_destination, command_bookmark, command_uri, destination_command,
    file_link, file_link_command, hwp_link, hwp_link_command, HwpLink, LinkDestinationKind,
    LINK_TARGET_EQUATION, LINK_TARGET_FIGURE, LINK_TARGET_OUTLINE, LINK_TARGET_TABLE,
};
use crate::model::paragraph::{FieldRange, Paragraph};
use crate::parser::tags;

/// 문단 주소. 빈 cell_path는 본문이며 각 경로 항목은 (컨트롤, 셀, 내부 문단)이다.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HyperlinkTarget {
    pub section: usize,
    pub para: usize,
    pub cell_path: Vec<(usize, usize, usize)>,
}

impl HyperlinkTarget {
    pub fn body(section: usize, para: usize) -> Self {
        Self {
            section,
            para,
            cell_path: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HyperlinkInfo {
    pub field_id: u32,
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub uri: String,
    /// 문서 내부 책갈피 링크의 대상 이름. `uri`에는 원문 주소(`?<이름>`)가 그대로 남는다.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bookmark: Option<String>,
    /// 한글 문서 링크(지금·다른 문서의 처음·책갈피·개요·표·그림·수식)의 대상.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hwp: Option<HwpLink>,
    /// 외부 파일 링크의 파일 경로.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

/// 한글 문서 링크가 가리킬 지금 문서의 개요 문단(`control` 없음)이나 표·그림·수식 컨트롤 자리.
/// 문단 주소는 [`HyperlinkTarget`]과 같다.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LinkObjectAddress {
    pub section: usize,
    pub para: usize,
    #[serde(default)]
    pub cell_path: Vec<(usize, usize, usize)>,
    #[serde(default)]
    pub control: Option<usize>,
}

impl DocumentCore {
    /// 지정 문단의 링크를 조회한다. 미지원 scheme도 원문 주소 그대로 반환한다.
    pub fn hyperlinks_native(
        &self,
        target: &HyperlinkTarget,
    ) -> Result<Vec<HyperlinkInfo>, HwpError> {
        let p = self.hyperlink_paragraph(target)?;
        Ok(p.field_ranges
            .iter()
            .filter_map(|range| {
                let Control::Field(field) = p.controls.get(range.control_idx)? else {
                    return None;
                };
                (field.field_type == FieldType::Hyperlink).then(|| HyperlinkInfo {
                    field_id: field.field_id,
                    start: range.start_char_idx,
                    end: range.end_char_idx,
                    text: p
                        .text
                        .chars()
                        .skip(range.start_char_idx)
                        .take(range.end_char_idx.saturating_sub(range.start_char_idx))
                        .collect(),
                    uri: command_uri(&field.command),
                    bookmark: command_bookmark(&field.command),
                    hwp: hwp_link(&field.command),
                    file: file_link(&field.command),
                })
            })
            .collect())
    }

    /// 선택한 표시 문자열에 웹 링크를 붙인다. 텍스트·글자 서식은 보존한다.
    /// 겹치는 필드와 빈 범위는 오류다. 무선택 삽입은 host가 텍스트 삽입과 함께 한 snapshot으로 묶는다.
    pub fn insert_hyperlink_native(
        &mut self,
        target: &HyperlinkTarget,
        start: usize,
        end: usize,
        uri: &str,
    ) -> Result<u32, HwpError> {
        self.insert_hyperlink_command(target, start, end, destination_command(uri)?)
    }

    /// [`Self::insert_hyperlink_native`] 에서 `mailto:` 주소를 웹 주소 꼴로 적는 변형.
    /// 한컴이 입력어 자동 실행으로 `mailto:` 를 직접 친 주소를 거는 꼴이다.
    pub fn insert_hyperlink_mail_as_web_native(
        &mut self,
        target: &HyperlinkTarget,
        start: usize,
        end: usize,
        uri: &str,
    ) -> Result<u32, HwpError> {
        let command = crate::model::hyperlink::destination_command_with_mail_form(uri, false)?;
        self.insert_hyperlink_command(target, start, end, command)
    }

    /// 선택한 표시 문자열에 문서 내부 책갈피 링크를 붙인다. 책갈피가 문서에 있어야 한다.
    pub fn insert_bookmark_hyperlink_native(
        &mut self,
        target: &HyperlinkTarget,
        start: usize,
        end: usize,
        bookmark: &str,
    ) -> Result<u32, HwpError> {
        let command = self.existing_bookmark_command(bookmark)?;
        self.insert_hyperlink_command(target, start, end, command)
    }

    /// 한컴 한글 문서 링크를 붙인다. 지금 문서(`link.path` 빈 값)의 개요·표·그림·수식 대상은
    /// `at`의 문단·컨트롤을 가리키며, 그 대상의 인스턴스 ID가 없거나 겹치면 한컴처럼 새로 매긴 뒤
    /// `#번호`로 잇는다. 지금 문서의 책갈피는 문서에 있어야 한다. 다른 문서 대상은 그대로 적는다.
    pub fn insert_hwp_hyperlink_native(
        &mut self,
        target: &HyperlinkTarget,
        start: usize,
        end: usize,
        link: &HwpLink,
        at: Option<&LinkObjectAddress>,
    ) -> Result<u32, HwpError> {
        let mut link = link.clone();
        let mut assign = None;
        if link.path.is_empty() && link.target_type != 0 {
            let at = at.ok_or_else(|| invalid("연결할 대상을 골라 주세요"))?;
            let (id, fresh) = self.link_object_id(link.target_type, at)?;
            link.anchor = format!("#{id}");
            assign = fresh.then_some((at.clone(), id));
        } else if link.path.is_empty()
            && !link.anchor.is_empty()
            && !self.has_bookmark_named(&link.anchor)
        {
            return Err(invalid("문서에 없는 책갈피입니다"));
        }
        let field_id =
            self.insert_hyperlink_command(target, start, end, hwp_link_command(&link)?)?;
        if let Some((at, id)) = assign {
            self.set_link_object_id(link.target_type, &at, id)?;
        }
        Ok(field_id)
    }

    /// 한컴 하이퍼링크 창 한글 문서 탭의 대상 목록: 개요 문단(번호·제목)과 표·그림·수식을 문서 순서로
    /// 주소·인스턴스 ID와 함께 준다. 링크를 따라갈 때도 `#번호`를 이 목록에서 찾는다.
    pub fn link_destinations_native(&self) -> serde_json::Value {
        use crate::document_core::queries::navigation::build_outline_navigation;
        let outline =
            build_outline_navigation(&self.document, &Default::default(), &Default::default());
        let outlines: Vec<_> = outline
            .outline
            .iter()
            .filter_map(|item| {
                let cell_path: Vec<(usize, usize, usize)> = item
                    .cell
                    .as_ref()
                    .map(|c| {
                        c.cell_path
                            .iter()
                            .map(|e| (e.control_index, e.cell_index, e.cell_para_index))
                            .collect()
                    })
                    .unwrap_or_default();
                let p = self
                    .hyperlink_paragraph(&HyperlinkTarget {
                        section: item.section,
                        para: item.paragraph,
                        cell_path: cell_path.clone(),
                    })
                    .ok()?;
                Some(serde_json::json!({
                    "section": item.section,
                    "para": item.paragraph,
                    "cellPath": cell_path,
                    "id": paragraph_instance_id(p),
                    "number": item.number,
                    "title": item.title,
                }))
            })
            .collect();
        let mut objects: [Vec<serde_json::Value>; 3] = Default::default();
        for (section, sec) in self.document.sections.iter().enumerate() {
            for (para, p) in sec.paragraphs.iter().enumerate() {
                collect_link_objects(p, section, para, &mut Vec::new(), &mut objects);
            }
        }
        let [tables, figures, equations] = objects;
        serde_json::json!({
            "outlines": outlines,
            "tables": tables,
            "figures": figures,
            "equations": equations,
        })
    }

    /// 외부 파일 링크(한컴 파일 탭의 기존 파일)를 붙인다.
    pub fn insert_file_hyperlink_native(
        &mut self,
        target: &HyperlinkTarget,
        start: usize,
        end: usize,
        path: &str,
    ) -> Result<u32, HwpError> {
        self.insert_hyperlink_command(target, start, end, file_link_command(path)?)
    }

    /// 대상의 인스턴스 ID와 새로 매겨야 하는지. 0이거나 문서의 다른 대상과 겹치면 새 번호다.
    fn link_object_id(
        &self,
        target_type: u8,
        at: &LinkObjectAddress,
    ) -> Result<(u32, bool), HwpError> {
        let mut ids = Vec::new();
        for sec in &self.document.sections {
            walk_paragraphs(&sec.paragraphs, &mut |p| {
                if target_type == LINK_TARGET_OUTLINE {
                    ids.push(paragraph_instance_id(p));
                } else {
                    ids.extend(p.controls.iter().filter_map(object_instance_id));
                }
            });
        }
        let current = self.link_object(target_type, at)?;
        let shared = ids.iter().filter(|&&id| id == current).count() > 1;
        if current != 0 && !shared {
            return Ok((current, false));
        }
        // 한컴은 큰 번호를 새로 매긴다. HWPX 문단 번호 발급기와 겹치지 않게 큰 수부터 쓴다.
        let next = ids.into_iter().max().unwrap_or(0).max(0x4000_0000) + 1;
        Ok((next, true))
    }

    /// 주소가 가리키는 대상의 지금 인스턴스 ID. 대상 종류와 맞지 않으면 오류다.
    fn link_object(&self, target_type: u8, at: &LinkObjectAddress) -> Result<u32, HwpError> {
        let p = self.hyperlink_paragraph(&HyperlinkTarget {
            section: at.section,
            para: at.para,
            cell_path: at.cell_path.clone(),
        })?;
        match (target_type, at.control) {
            (LINK_TARGET_OUTLINE, None) => {
                let is_outline = self
                    .document
                    .doc_info
                    .para_shapes
                    .get(p.para_shape_id as usize)
                    .is_some_and(|shape| shape.head_type == crate::model::style::HeadType::Outline);
                if !is_outline {
                    return Err(invalid("개요 문단이 아닙니다"));
                }
                Ok(paragraph_instance_id(p))
            }
            (_, Some(idx)) => match (target_type, p.controls.get(idx)) {
                (LINK_TARGET_TABLE, Some(c @ Control::Table(_)))
                | (LINK_TARGET_FIGURE, Some(c @ Control::Picture(_)))
                | (LINK_TARGET_EQUATION, Some(c @ Control::Equation(_))) => {
                    Ok(object_instance_id(c).unwrap_or(0))
                }
                _ => Err(invalid("연결할 개체를 찾을 수 없습니다")),
            },
            _ => Err(invalid("연결할 대상을 골라 주세요")),
        }
    }

    fn set_link_object_id(
        &mut self,
        target_type: u8,
        at: &LinkObjectAddress,
        id: u32,
    ) -> Result<(), HwpError> {
        let p = if at.cell_path.is_empty() {
            self.document
                .sections
                .get_mut(at.section)
                .and_then(|s| s.paragraphs.get_mut(at.para))
                .ok_or_else(|| invalid("문단 인덱스 초과"))?
        } else {
            self.get_cell_paragraph_mut_by_path(at.section, at.para, &at.cell_path)?
        };
        if target_type == LINK_TARGET_OUTLINE {
            // raw_header_extra[6..10]이 HWP5 PARA_HEADER instanceId이자 HWPX `hp:p id`다.
            if p.raw_header_extra.len() < 10 {
                p.raw_header_extra.resize(10, 0);
            }
            p.raw_header_extra[6..10].copy_from_slice(&id.to_le_bytes());
        } else {
            match at.control.and_then(|idx| p.controls.get_mut(idx)) {
                Some(Control::Table(t)) => t.common.instance_id = id,
                Some(Control::Picture(pic)) => pic.common.instance_id = id,
                Some(Control::Equation(eq)) => eq.common.instance_id = id,
                _ => return Err(invalid("연결할 개체를 찾을 수 없습니다")),
            }
        }
        self.document.sections[at.section].raw_stream = None;
        self.mark_section_dirty(at.section);
        Ok(())
    }

    fn insert_hyperlink_command(
        &mut self,
        target: &HyperlinkTarget,
        start: usize,
        end: usize,
        command: String,
    ) -> Result<u32, HwpError> {
        self.validate_hyperlink_paragraph(target)?;
        let mut candidate = self.hyperlink_paragraph(target)?.clone();
        let len = candidate.text.chars().count();
        if start >= end || end > len {
            return Err(invalid(
                "하이퍼링크는 문단 안의 비어 있지 않은 범위가 필요합니다",
            ));
        }
        if candidate.field_ranges.iter().any(|r| {
            start < r.end_char_idx && r.start_char_idx < end
                || (r.start_char_idx == r.end_char_idx
                    && start <= r.start_char_idx
                    && r.start_char_idx < end)
        }) {
            return Err(invalid("기존 필드와 겹치는 하이퍼링크는 지원하지 않습니다"));
        }
        let mut max_id = 0;
        for s in &self.document.sections {
            for p in &s.paragraphs {
                super::queries::field_query::collect_max_field_id(p, &mut max_id);
            }
        }
        let field_id = max_id
            .checked_add(1)
            .ok_or_else(|| invalid("필드 ID 공간이 소진되었습니다"))?;
        // FieldRange가 정확한 위치 정본이다. END 갭을 controls[] 슬롯으로 오인하지 않는다.
        let positions = candidate.control_text_positions();
        let insert_idx = candidate
            .controls
            .iter()
            .enumerate()
            .position(|(idx, _)| {
                candidate
                    .field_ranges
                    .iter()
                    .find(|r| r.control_idx == idx)
                    .map_or(positions[idx], |r| r.start_char_idx)
                    > start
            })
            .unwrap_or(candidate.controls.len());
        let original_format = crate::model::hyperlink_format::OriginalFormat::capture(
            &candidate,
            &self.document.doc_info.char_shapes,
            start,
            end,
        );
        let end_raw = raw_boundary(&candidate, end);
        let start_raw = raw_boundary(&candidate, start);
        shift_axis(&mut candidate, end_raw, 8)?;
        shift_axis(&mut candidate, start_raw, 8)?;
        for r in &mut candidate.field_ranges {
            if r.control_idx >= insert_idx {
                r.control_idx += 1;
            }
        }
        candidate.controls.insert(
            insert_idx,
            Control::Field(Field {
                field_type: FieldType::Hyperlink,
                command,
                ctrl_id: tags::FIELD_HYPERLINK,
                field_id,
                hyperlink_format: original_format.map(Box::new),
                ..Default::default()
            }),
        );
        candidate
            .ctrl_data_records
            .resize(candidate.controls.len() - 1, None);
        candidate.ctrl_data_records.insert(insert_idx, None);
        candidate.field_ranges.push(FieldRange {
            start_char_idx: start,
            end_char_idx: end,
            control_idx: insert_idx,
            ..Default::default()
        });
        candidate
            .field_ranges
            .sort_by_key(|r| (r.start_char_idx, r.control_idx));
        self.commit_hyperlink_paragraph(target, candidate, field_id)?;
        Ok(field_id)
    }

    /// 주소만 바꾼다. 동일 주소는 캐시/이벤트를 건드리지 않고 false를 반환한다.
    pub fn update_hyperlink_native(
        &mut self,
        target: &HyperlinkTarget,
        field_id: u32,
        uri: &str,
    ) -> Result<bool, HwpError> {
        let command = destination_command(uri)?;
        self.update_hyperlink_command(target, field_id, command, Some(uri))
    }

    /// 책갈피 링크의 대상 책갈피만 바꾼다. 동일 대상은 false를 반환한다.
    pub fn update_bookmark_hyperlink_native(
        &mut self,
        target: &HyperlinkTarget,
        field_id: u32,
        bookmark: &str,
    ) -> Result<bool, HwpError> {
        let command = self.existing_bookmark_command(bookmark)?;
        self.update_hyperlink_command(target, field_id, command, None)
    }

    /// `path`는 웹 링크의 `Path` 파라미터 값이다. 책갈피 링크는 `Path`를 쓰지 않는다.
    fn update_hyperlink_command(
        &mut self,
        target: &HyperlinkTarget,
        field_id: u32,
        command: String,
        path: Option<&str>,
    ) -> Result<bool, HwpError> {
        self.validate_hyperlink_paragraph(target)?;
        let p = self.hyperlink_paragraph(target)?;
        let (_, idx) = find_link(p, field_id)?;
        let Control::Field(field) = &p.controls[idx] else {
            unreachable!()
        };
        // 링크 종류(웹/책갈피)를 암묵적으로 바꾸지 않는다. Category/TargetType 파라미터가 어긋난다.
        // 외부 목적지는 종류를 보존하고 책갈피 링크와 구별한다.
        let same_kind = match path {
            Some(_) => {
                destination_command(&command_uri(&field.command)).is_ok()
                    && command_bookmark(&field.command).is_none()
            }
            None => command_bookmark(&field.command).is_some(),
        };
        if !same_kind {
            return Err(invalid(
                "다른 종류의 하이퍼링크는 이 주소로 수정할 수 없습니다",
            ));
        }
        if command_uri(&field.command) == command_uri(&command) {
            return Ok(false);
        }
        let mut candidate = p.clone();
        let Control::Field(field) = &mut candidate.controls[idx] else {
            unreachable!()
        };
        field.command = command.clone();
        field.raw_parameters_xml = None;
        let mut has_command = false;
        for item in &mut field.parameters.items {
            if let Parameter::String {
                name: Some(name),
                value,
                ..
            } = item
            {
                match name.as_str() {
                    "Command" => {
                        *value = command.clone();
                        has_command = true;
                    }
                    "Path" => {
                        if let Some(path) = path {
                            *value = path.to_string();
                        }
                    }
                    // 한컴은 웹 주소를 URL, 메일 주소를 EMAIL 종류로 적는다. 둘 사이로 바뀌면 따라 고친다.
                    "Category"
                        if matches!(
                            value.as_str(),
                            "HWPHYPERLINK_TYPE_URL" | "HWPHYPERLINK_TYPE_EMAIL"
                        ) =>
                    {
                        match path.and_then(classify_destination) {
                            Some(LinkDestinationKind::Web) => {
                                *value = "HWPHYPERLINK_TYPE_URL".into()
                            }
                            Some(LinkDestinationKind::Mail) => {
                                *value = "HWPHYPERLINK_TYPE_EMAIL".into()
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        if !has_command && !field.parameters.is_empty() {
            field.parameters.items.push(Parameter::String {
                name: Some("Command".into()),
                value: command,
                preserve_space: false,
            });
        }
        // HWP5 command는 CTRL_HEADER에 따로 저장된다. 해석하지 않는 CTRL_DATA는 보존한다.
        self.commit_hyperlink_paragraph(target, candidate, field_id)?;
        Ok(true)
    }

    /// 표시 문자열만 교체한다. 필드 ID·주소·부가 정보와 인접 필드 범위를 보존한다.
    pub fn replace_hyperlink_text_native(
        &mut self,
        target: &HyperlinkTarget,
        field_id: u32,
        text: &str,
    ) -> Result<bool, HwpError> {
        if text.trim().is_empty() || text.chars().any(char::is_control) {
            return Err(invalid("표시할 글자를 한 줄로 입력해 주세요"));
        }
        self.validate_hyperlink_paragraph(target)?;
        let mut candidate = self.hyperlink_paragraph(target)?.clone();
        let (range_idx, ctrl_idx) = find_link(&candidate, field_id)?;
        let mut original_format = match &candidate.controls[ctrl_idx] {
            Control::Field(f) => f.hyperlink_format.clone(),
            _ => None,
        };
        let ranges = candidate.field_ranges.clone();
        let range = &ranges[range_idx];
        let start = range.start_char_idx;
        let end = range.end_char_idx;
        if ranges
            .iter()
            .enumerate()
            .any(|(idx, r)| idx != range_idx && start < r.end_char_idx && r.start_char_idx < end)
        {
            return Err(invalid("겹치는 필드의 표시 문자열은 수정할 수 없습니다"));
        }
        let old: String = candidate
            .text
            .chars()
            .skip(start)
            .take(end - start)
            .collect();
        if old == text {
            return Ok(false);
        }
        let raw_start = raw_boundary(&candidate, start);
        let shape = candidate
            .char_shapes
            .iter()
            .rev()
            .find(|s| s.start_pos <= raw_start)
            .cloned();
        let new_len = text.chars().count();
        // 다른 컨트롤은 바꾼 글자를 기준으로 옮긴다 — 지운 글자 사이·뒤 갭의 컨트롤은 새 글자 뒤로 간다.
        let positions: Vec<usize> = candidate
            .control_text_positions()
            .into_iter()
            .map(|at| match at {
                _ if at <= start => at,
                _ if at <= end => start + new_len,
                _ => at - (end - start) + new_len,
            })
            .collect();
        // 먼저 삽입하여 빈 필드로 축소되는 중간 상태를 피한다.
        candidate.insert_text_at(start, text);
        candidate.delete_text_at(start + new_len, end - start);
        // 경계 삽입의 일반 규칙이 이전 링크 끝까지 늘리지 않도록 정본에서 복원한다.
        for (idx, original) in ranges.into_iter().enumerate() {
            let mut adjusted = original;
            if idx == range_idx {
                adjusted.end_char_idx = start + new_len;
            } else if adjusted.start_char_idx >= end {
                adjusted.start_char_idx = adjusted.start_char_idx - (end - start) + new_len;
                adjusted.end_char_idx = adjusted.end_char_idx - (end - start) + new_len;
            }
            candidate.field_ranges[idx] = adjusted;
        }
        if let Some(format) = &mut original_format {
            format.repeat_first(new_len);
        }
        if let Control::Field(f) = &mut candidate.controls[ctrl_idx] {
            f.hyperlink_format = original_format;
        }
        candidate.rebuild_char_offsets(&positions);
        // 새 글자는 기존 링크 첫 글자의 서식을 이어받는다.
        if let Some(mut shape) = shape {
            let new_start = raw_boundary(&candidate, start);
            shape.start_pos = new_start;
            candidate.char_shapes.retain(|s| s.start_pos != new_start);
            candidate.char_shapes.push(shape);
            candidate.char_shapes.sort_by_key(|s| s.start_pos);
        }
        candidate.stored_text_partition_dirty = true;
        self.commit_hyperlink_paragraph(target, candidate, field_id)?;
        Ok(true)
    }

    /// 링크의 시작/끝 마커만 제거한다. 표시 문자열과 글자 서식은 남긴다.
    pub fn remove_hyperlink_native(
        &mut self,
        target: &HyperlinkTarget,
        field_id: u32,
    ) -> Result<(), HwpError> {
        self.remove_hyperlink_with_format_native(target, field_id, false)
    }

    /// Studio 속성 해제: 링크로 덮은 색/밑줄만 복원한다. 정보 없는 외부 링크는 현재 서식 유지.
    pub fn remove_hyperlink_with_format_native(
        &mut self,
        target: &HyperlinkTarget,
        field_id: u32,
        restore_format: bool,
    ) -> Result<(), HwpError> {
        self.validate_hyperlink_paragraph(target)?;
        let mut candidate = self.hyperlink_paragraph(target)?.clone();
        let (range_idx, ctrl_idx) = find_link(&candidate, field_id)?;
        let range = candidate.field_ranges[range_idx].clone();
        let formats = match &candidate.controls[ctrl_idx] {
            Control::Field(f) if restore_format => f
                .hyperlink_format
                .as_ref()
                .filter(|f| f.valid_for(range.end_char_idx - range.start_char_idx))
                .map(|f| f.edits(range.start_char_idx))
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let end_raw = raw_boundary(&candidate, range.end_char_idx);
        let start_raw = if range.start_char_idx == range.end_char_idx {
            end_raw
                .checked_sub(8)
                .ok_or_else(|| invalid("빈 필드 마커 좌표 오류"))?
        } else {
            raw_boundary(&candidate, range.start_char_idx)
        };
        if start_raw < 8 || end_raw.saturating_sub(start_raw) < 8 {
            return Err(invalid("필드 마커 좌표가 올바르지 않습니다"));
        }
        shift_axis(&mut candidate, end_raw, -8)?;
        shift_axis(&mut candidate, start_raw, -8)?;
        candidate.field_ranges.remove(range_idx);
        candidate.controls.remove(ctrl_idx);
        if ctrl_idx < candidate.ctrl_data_records.len() {
            candidate.ctrl_data_records.remove(ctrl_idx);
        }
        for range in &mut candidate.field_ranges {
            if range.control_idx > ctrl_idx {
                range.control_idx -= 1;
            }
        }
        candidate.forget_para_text_if_emptied();
        for (start, end, mods) in formats {
            let ids = self
                .document
                .modified_char_shape_ids(candidate.char_shape_ids_in_range(start, end), &mods);
            candidate.try_map_char_shape_range(start, end, |id| {
                ids.get(&id)
                    .copied()
                    .ok_or_else(|| invalid("원래 링크 글자 모양을 복원할 수 없습니다"))
            })?;
        }
        self.rebuild_resolved_styles();
        self.commit_hyperlink_paragraph(target, candidate, field_id)?;
        Ok(())
    }

    fn existing_bookmark_command(&self, bookmark: &str) -> Result<String, HwpError> {
        let command = bookmark_command(bookmark)?;
        if !self.has_bookmark_named(bookmark) {
            return Err(invalid("문서에 없는 책갈피입니다"));
        }
        Ok(command)
    }

    pub(crate) fn hyperlink_paragraph(
        &self,
        target: &HyperlinkTarget,
    ) -> Result<&Paragraph, HwpError> {
        let (paragraphs, idx) = self.hyperlink_paragraph_list(target)?;
        paragraphs
            .get(idx)
            .ok_or_else(|| invalid("문단 인덱스 초과"))
    }

    fn hyperlink_paragraph_list(
        &self,
        target: &HyperlinkTarget,
    ) -> Result<(&[Paragraph], usize), HwpError> {
        let mut paragraphs = self
            .document
            .sections
            .get(target.section)
            .ok_or_else(|| invalid("구역 인덱스 초과"))?
            .paragraphs
            .as_slice();
        let mut para_idx = target.para;
        for &(ctrl_idx, cell_idx, child_para) in &target.cell_path {
            let p = paragraphs
                .get(para_idx)
                .ok_or_else(|| invalid("상위 문단 인덱스 초과"))?;
            paragraphs = p
                .controls
                .get(ctrl_idx)
                .and_then(|control| path_paragraphs(control, cell_idx))
                .ok_or_else(|| invalid("지원하지 않는 셀/글상자 경로"))?;
            para_idx = child_para;
        }
        Ok((paragraphs, para_idx))
    }

    /// 문서의 모든 링크 문단 주소. 본문·표 셀·글상자·캡션·머리말·꼬리말·각주·미주를
    /// 문서 순서로 훑어 하이퍼링크 필드가 범위로 든 문단만 돌려준다(한컴 하이퍼링크
    /// 글자 모양의 문서 전체 적용 대상). 주소는 [`Self::hyperlinks_native`]가 받는 꼴이다.
    pub fn hyperlink_targets_native(&self) -> Vec<HyperlinkTarget> {
        let mut out = Vec::new();
        for (section, sec) in self.document.sections.iter().enumerate() {
            for (para, p) in sec.paragraphs.iter().enumerate() {
                collect_hyperlink_targets(p, section, para, &mut Vec::new(), &mut out);
            }
        }
        out
    }

    fn validate_hyperlink_paragraph(&self, target: &HyperlinkTarget) -> Result<(), HwpError> {
        let p = self.hyperlink_paragraph(target)?;
        if p.field_ranges.iter().any(|r| {
            r.start_char_idx > r.end_char_idx
                || r.end_char_idx > p.text.chars().count()
                || r.control_idx >= p.controls.len()
        }) {
            return Err(invalid("필드 범위가 문단과 일치하지 않습니다"));
        }
        if self.document.header.version.major == 3
            || !p.orphan_field_ends.is_empty()
            || !p.title_marks.is_empty()
            || p.controls.iter().enumerate().any(|(idx, c)| match c {
                Control::SectionDef(_) | Control::ColumnDef(_) => false,
                Control::Field(_) => !p.field_ranges.iter().any(|r| {
                    r.control_idx == idx
                        && r.start_char_idx <= r.end_char_idx
                        && r.inner_slot_count == 0
                }),
                _ => true,
            })
            || p.char_offsets.len() != p.text.chars().count()
        {
            return Err(invalid(
                "개체·다단락 필드·HWP3 문단의 링크 편집은 아직 지원하지 않습니다",
            ));
        }
        // 앞 문단에서 열린 다단락 필드의 중간 문단도 편집 대상에서 제외한다.
        let mut open = std::collections::HashSet::new();
        let (paragraphs, para_idx) = self.hyperlink_paragraph_list(target)?;
        for preceding in &paragraphs[..para_idx] {
            for (idx, c) in preceding.controls.iter().enumerate() {
                if let Control::Field(f) = c {
                    if !preceding.field_ranges.iter().any(|r| r.control_idx == idx) {
                        open.insert(f.field_id);
                    }
                }
            }
            for end in &preceding.orphan_field_ends {
                open.remove(&end.begin_id_ref);
            }
        }
        if !open.is_empty() {
            return Err(invalid(
                "다단락 필드 안의 링크 편집은 아직 지원하지 않습니다",
            ));
        }
        Ok(())
    }

    fn commit_hyperlink_paragraph(
        &mut self,
        target: &HyperlinkTarget,
        candidate: Paragraph,
        field_id: u32,
    ) -> Result<(), HwpError> {
        let section = target.section;
        let para = target.para;
        let axis_changed = candidate.stored_text_partition_dirty;
        if target.cell_path.is_empty() {
            let stored_end = crate::renderer::composer::paragraph_flow_end(
                &self.document.sections[section].paragraphs[para],
            );
            self.document.sections[section].paragraphs[para] = candidate;
            if axis_changed {
                self.reflow_paragraph(section, para);
                let hwp3_layout = self.document.layout_profile().hwp3_layout();
                crate::renderer::composer::recalculate_section_vpos(
                    &mut self.document.sections[section].paragraphs,
                    para,
                    None,
                    stored_end,
                    &self.styles,
                    self.dpi,
                    hwp3_layout,
                );
                self.recompose_paragraph(section, para);
            }
        } else {
            *self.get_cell_paragraph_mut_by_path(section, para, &target.cell_path)? = candidate;
            let path = &target.cell_path;
            if axis_changed {
                let inner_para = path.last().unwrap().2;
                self.reflow_cell_paragraph_by_path(section, para, path, inner_para);
                self.recalculate_cell_paragraph_vpos_by_path(section, para, path, inner_para, None);
            }
            self.mark_cell_control_dirty(section, para, path[0].0);
        }
        self.document.sections[section].raw_stream = None;
        self.mark_section_dirty(section);
        self.paginate_if_needed();
        self.invalidate_page_tree_cache();
        self.event_log.push(DocumentEvent::HyperlinkChanged {
            section,
            para,
            cell_path: target.cell_path.clone(),
            field_id,
        });
        Ok(())
    }
}

/// 경로 항목 (컨트롤, 셀) 이 가리키는 문단 목록. 셀 65534는 표·도형 캡션, 그림 캡션·
/// 머리말·꼬리말·각주·미주는 셀 0이다(`get_cell_paragraphs_mut_by_path`와 같은 꼴).
fn path_paragraphs(control: &Control, cell_idx: usize) -> Option<&[Paragraph]> {
    match control {
        Control::Table(table) if cell_idx == 65534 => Some(&table.caption.as_ref()?.paragraphs),
        Control::Table(table) => Some(&table.cells.get(cell_idx)?.paragraphs),
        Control::Shape(shape) if cell_idx == 65534 => {
            Some(&DocumentCore::shape_caption_ref(shape)?.paragraphs)
        }
        Control::Shape(shape) => Some(&super::helpers::get_textbox_at(shape, cell_idx)?.paragraphs),
        Control::Picture(picture) if cell_idx == 0 => Some(&picture.caption.as_ref()?.paragraphs),
        Control::Footnote(note) if cell_idx == 0 => Some(&note.paragraphs),
        Control::Endnote(note) if cell_idx == 0 => Some(&note.paragraphs),
        _ if cell_idx == 0 => control.header_footer_paragraphs().map(Vec::as_slice),
        _ => None,
    }
}

/// 컨트롤이 품은 문단 목록들(표 셀·글상자·캡션·머리말·꼬리말·각주·미주)과 그 경로 셀 번호.
fn child_lists(control: &Control) -> Vec<(usize, &[Paragraph])> {
    let cells: Vec<usize> = match control {
        Control::Table(table) => (0..table.cells.len()).chain([65534]).collect(),
        Control::Shape(shape) => (0..shape.textbox_shape_count()).chain([65534]).collect(),
        _ => vec![0],
    };
    cells
        .into_iter()
        .filter_map(|cell| Some((cell, path_paragraphs(control, cell)?)))
        .collect()
}

/// 문단과 그 안 문단을 문서 순서로 모두 훑는다.
pub(crate) fn walk_paragraphs<'a>(paragraphs: &'a [Paragraph], f: &mut dyn FnMut(&'a Paragraph)) {
    for p in paragraphs {
        f(p);
        for control in &p.controls {
            for (_, inner) in child_lists(control) {
                walk_paragraphs(inner, f);
            }
        }
    }
}

/// 문단 `p`와 그 안의 표·그림·수식을 [표, 그림, 수식] 목록에 문서 순서로 더한다.
fn collect_link_objects(
    p: &Paragraph,
    section: usize,
    para: usize,
    path: &mut Vec<(usize, usize, usize)>,
    out: &mut [Vec<serde_json::Value>; 3],
) {
    for (ctrl_idx, control) in p.controls.iter().enumerate() {
        let kind = match control {
            Control::Table(_) => Some(0),
            Control::Picture(_) => Some(1),
            Control::Equation(_) => Some(2),
            _ => None,
        };
        if let Some(kind) = kind {
            out[kind].push(serde_json::json!({
                "section": section,
                "para": para,
                "cellPath": path.clone(),
                "control": ctrl_idx,
                "id": object_instance_id(control).unwrap_or(0),
            }));
        }
        for (cell_idx, paragraphs) in child_lists(control) {
            for (child, inner) in paragraphs.iter().enumerate() {
                path.push((ctrl_idx, cell_idx, child));
                collect_link_objects(inner, section, para, path, out);
                path.pop();
            }
        }
    }
}

/// 문단 인스턴스 ID(HWP5 PARA_HEADER instanceId, HWPX `hp:p id`). 없으면 0이다.
pub(crate) fn paragraph_instance_id(p: &Paragraph) -> u32 {
    p.raw_header_extra
        .get(6..10)
        .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// 링크 대상이 되는 개체(표·그림·도형·수식)의 인스턴스 ID.
fn object_instance_id(control: &Control) -> Option<u32> {
    match control {
        Control::Table(t) => Some(t.common.instance_id),
        Control::Picture(p) => Some(p.common.instance_id),
        Control::Equation(e) => Some(e.common.instance_id),
        Control::Shape(s) => Some(s.common().instance_id),
        _ => None,
    }
}

/// 문서의 개요 링크(`?#번호;0;1;…`, 지금 문서)가 가리키는 문단 인스턴스 ID. HWPX 저장이 이
/// 문단의 `hp:p id`를 지켜야 링크가 산다.
pub(crate) fn linked_outline_ids(
    doc: &crate::model::document::Document,
) -> std::collections::HashSet<u32> {
    let mut ids = std::collections::HashSet::new();
    for sec in &doc.sections {
        walk_paragraphs(&sec.paragraphs, &mut |p| {
            for c in &p.controls {
                let Control::Field(f) = c else { continue };
                if let Some(link) = hwp_link(&f.command) {
                    if link.path.is_empty() && link.target_type == LINK_TARGET_OUTLINE {
                        if let Some(id) = link.anchor.strip_prefix('#').and_then(|n| n.parse().ok())
                        {
                            ids.insert(id);
                        }
                    }
                }
            }
        });
    }
    ids
}

/// 문단 `p`에 링크가 있으면 그 주소를 더하고, 컨트롤 안 문단 목록으로 내려간다.
fn collect_hyperlink_targets(
    p: &Paragraph,
    section: usize,
    para: usize,
    path: &mut Vec<(usize, usize, usize)>,
    out: &mut Vec<HyperlinkTarget>,
) {
    if p.field_ranges.iter().any(|r| {
        matches!(
            p.controls.get(r.control_idx),
            Some(Control::Field(field)) if field.field_type == FieldType::Hyperlink
        )
    }) {
        out.push(HyperlinkTarget {
            section,
            para,
            cell_path: path.clone(),
        });
    }
    for (ctrl_idx, control) in p.controls.iter().enumerate() {
        for (cell_idx, paragraphs) in child_lists(control) {
            // 경로 마지막 항목의 문단 번호가 지금 문단 `p`다. 그 컨트롤 안으로 한 겹 더 간다.
            for (child, inner) in paragraphs.iter().enumerate() {
                path.push((ctrl_idx, cell_idx, child));
                collect_hyperlink_targets(inner, section, para, path, out);
                path.pop();
            }
        }
    }
}

fn invalid(message: &str) -> HwpError {
    HwpError::InvalidField(message.into())
}

fn find_link(p: &Paragraph, id: u32) -> Result<(usize, usize), HwpError> {
    p.field_ranges
        .iter()
        .enumerate()
        .find_map(|(idx, r)| match p.controls.get(r.control_idx) {
            Some(Control::Field(f)) if f.field_type == FieldType::Hyperlink && f.field_id == id => {
                Some((idx, r.control_idx))
            }
            _ => None,
        })
        .ok_or_else(|| invalid("하이퍼링크 필드를 찾을 수 없습니다"))
}

fn raw_boundary(p: &Paragraph, offset: usize) -> u32 {
    p.char_offsets
        .get(offset)
        .copied()
        .unwrap_or(p.char_count.saturating_sub(1))
}

/// 8-unit 마커를 넣거나 빼며 모든 raw UTF-16 참조 축을 함께 옮긴다.
/// 첫 글자 서식의 0 anchor는 그대로 둔다. 표시 문자 인덱스는 변하지 않는다.
fn shift_axis(p: &mut Paragraph, boundary: u32, delta: i32) -> Result<(), HwpError> {
    let shift = |v: &mut u32| -> Result<(), HwpError> {
        if *v >= boundary {
            *v = v
                .checked_add_signed(delta)
                .ok_or_else(|| invalid("필드 좌표 범위 초과"))?;
        }
        Ok(())
    };
    for v in &mut p.char_offsets {
        shift(v)?;
    }
    for shape in &mut p.char_shapes {
        if shape.start_pos != 0 {
            shift(&mut shape.start_pos)?;
        }
    }
    for range in &mut p.range_tags {
        shift(&mut range.start)?;
        shift(&mut range.end)?;
    }
    // 기존 저장 줄 경계는 HWPX 별도 축일 수 있으므로 재조판한다.
    p.stored_text_partition_dirty = true;
    p.char_count = p
        .char_count
        .checked_add_signed(delta)
        .ok_or_else(|| invalid("문단 길이 범위 초과"))?;
    Ok(())
}
