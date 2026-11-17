//! 숨은 설명(`tcmt`) 넣기·지우기와 따로 편집하기.
//!
//! 한컴은 숨은 설명을 "<<숨은 설명>>" 편집 화면에서 따로 고친다. 그 화면은 숨은 설명의 문단만 든
//! 문서처럼 다루면 되므로, 숨은 설명 문단을 같은 서식표의 따로 문서로 꺼내고(`hidden_comment_document`)
//! 고친 문서의 문단을 이 문서 서식표로 옮겨 되돌린다(`set_hidden_comment_from`).
//! 자리는 본문 문단과 셀 경로다(빈 경로면 본문).

use crate::document_core::DocumentCore;
use crate::error::HwpError;
use crate::model::control::{Control, HiddenComment};
use crate::model::document::{Document, Section};
use crate::model::paragraph::{CharShapeRef, Paragraph};

type Step = (usize, usize, usize);

/// 편집 화면의 쪽 여백. 한컴 12.30 숨은 설명 화면처럼 용지 가장자리 가까이 쓴다(5mm).
const EDIT_MARGIN: u32 = 1417;

impl DocumentCore {
    fn hidden_comment_host(
        &self,
        section: usize,
        para: usize,
        path: &[Step],
    ) -> Result<&Paragraph, HwpError> {
        if path.is_empty() {
            self.document
                .sections
                .get(section)
                .and_then(|s| s.paragraphs.get(para))
                .ok_or_else(|| HwpError::RenderError("숨은 설명 문단을 찾을 수 없습니다".into()))
        } else {
            self.resolve_paragraph_by_path(section, para, path)
        }
    }

    fn hidden_comment_at(
        &self,
        section: usize,
        para: usize,
        path: &[Step],
        control: usize,
    ) -> Result<&HiddenComment, HwpError> {
        match self
            .hidden_comment_host(section, para, path)?
            .controls
            .get(control)
        {
            Some(Control::HiddenComment(comment)) => Ok(comment),
            _ => Err(HwpError::RenderError(
                "그 자리에 숨은 설명이 없습니다".into(),
            )),
        }
    }

    /// 바탕글 스타일의 빈 문단. 한컴 12.30이 새 숨은 설명에 두는 문단이다.
    fn hidden_comment_paragraph(&self) -> Paragraph {
        let (para_shape_id, char_shape_id) = self
            .document
            .doc_info
            .styles
            .first()
            .map_or((0, 0), |style| (style.para_shape_id, style.char_shape_id));
        Paragraph {
            para_shape_id,
            style_id: 0,
            char_shapes: vec![CharShapeRef {
                start_pos: 0,
                char_shape_id: char_shape_id.into(),
            }],
            ..Paragraph::new_empty()
        }
    }

    /// 캐럿 자리(문단의 글자 번호)에 빈 숨은 설명을 넣는다.
    /// 반환: `{"ok":true,"controlIdx":N}`.
    pub fn insert_hidden_comment_native(
        &mut self,
        section: usize,
        para: usize,
        path: &[Step],
        char_offset: usize,
    ) -> Result<String, HwpError> {
        let inner = self.hidden_comment_paragraph();
        let host = self.code_paragraph_mut(section, para, path)?;
        let positions = crate::document_core::helpers::find_control_text_positions(host);
        let index = positions
            .iter()
            .position(|&pos| pos > char_offset)
            .unwrap_or(host.controls.len());
        host.align_ctrl_data_records();
        host.controls.insert(
            index,
            Control::HiddenComment(Box::new(HiddenComment {
                paragraphs: vec![inner],
            })),
        );
        host.ctrl_data_records.insert(index, None);
        host.shift_for_inline_control_insert(index, char_offset);
        host.char_count += 8;
        host.control_mask |= 1u32 << 0x000F;
        host.has_para_text = true;
        host.invalidate_layout_inputs();
        if path.is_empty() {
            self.shift_active_field_for_control_insert(section, para, index);
        }
        self.document.sections[section].raw_stream = None;
        self.reflow_hidden_comment_host(section, para, path);
        Ok(crate::document_core::helpers::json_ok_with(&format!(
            "\"controlIdx\":{index}"
        )))
    }

    /// 숨은 설명을 그 안의 내용째 지운다.
    pub fn delete_hidden_comment_native(
        &mut self,
        section: usize,
        para: usize,
        path: &[Step],
        control: usize,
    ) -> Result<String, HwpError> {
        self.hidden_comment_at(section, para, path, control)?;
        let host = self.code_paragraph_mut(section, para, path)?;
        host.remove_control(control);
        host.invalidate_layout_inputs();
        self.document.sections[section].raw_stream = None;
        self.reflow_hidden_comment_host(section, para, path);
        Ok("{\"ok\":true}".to_string())
    }

    fn reflow_hidden_comment_host(&mut self, section: usize, para: usize, path: &[Step]) {
        let (body, nested) = if path.is_empty() {
            (vec![(section, para)], Vec::new())
        } else {
            (Vec::new(), vec![(section, para, path.to_vec())])
        };
        self.reflow_after_code_erase(body, nested, &[section]);
        self.paginate_if_needed();
        self.invalidate_page_tree_cache();
    }

    /// 숨은 설명의 문단만 든 따로 문서. 서식표와 그림은 이 문서 것을 그대로 쓰고, 쪽은 그 구역의 용지에
    /// 머리말·꼬리말·바탕쪽·쪽 테두리 없이 좁은 여백을 둔다.
    pub fn hidden_comment_document(
        &self,
        section: usize,
        para: usize,
        path: &[Step],
        control: usize,
    ) -> Result<DocumentCore, HwpError> {
        let comment = self.hidden_comment_at(section, para, path, control)?;
        let mut section_def = self.document.sections[section].section_def.clone();
        section_def.master_pages.clear();
        section_def.hide_header = true;
        section_def.hide_footer = true;
        section_def.hide_master_page = true;
        section_def.hide_border = true;
        section_def.hide_fill = true;
        let page = &mut section_def.page_def;
        page.margin_left = EDIT_MARGIN;
        page.margin_right = EDIT_MARGIN;
        page.margin_top = EDIT_MARGIN;
        page.margin_bottom = EDIT_MARGIN;
        page.margin_header = 0;
        page.margin_footer = 0;
        page.margin_gutter = 0;
        let mut paragraphs = comment.paragraphs.clone();
        if paragraphs.is_empty() {
            paragraphs.push(self.hidden_comment_paragraph());
        }
        let source = &self.document;
        let document = Document {
            header: source.header.clone(),
            doc_properties: source.doc_properties.clone(),
            doc_info: source.doc_info.clone(),
            bin_data_content: source.bin_data_content.clone(),
            sections: vec![Section {
                section_def,
                paragraphs,
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut core = DocumentCore::new_empty();
        core.set_document(document);
        Ok(core)
    }

    /// `source`(숨은 설명 편집 문서)의 문단으로 숨은 설명 내용을 바꾼다. 새 글자·문단 모양은 이 문서 서식표에 옮긴다.
    pub fn set_hidden_comment_from(
        &mut self,
        section: usize,
        para: usize,
        path: &[Step],
        control: usize,
        source: &Document,
    ) -> Result<String, HwpError> {
        self.hidden_comment_at(section, para, path, control)?;
        let mut paragraphs = self.import_foreign_paragraphs(source);
        if paragraphs.is_empty() {
            paragraphs.push(self.hidden_comment_paragraph());
        }
        let host = self.code_paragraph_mut(section, para, path)?;
        if let Some(Control::HiddenComment(comment)) = host.controls.get_mut(control) {
            comment.paragraphs = paragraphs;
        }
        self.document.sections[section].raw_stream = None;
        self.invalidate_page_tree_cache();
        Ok("{\"ok\":true}".to_string())
    }
}
