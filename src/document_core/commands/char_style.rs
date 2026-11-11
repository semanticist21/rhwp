//! 글자 스타일을 글자 범위에 주거나 푼다. 한컴처럼 글자 모양과 함께 글자별 글자 스타일
//! 참조(`model::paragraph::char_style`)를 고쳐, 스타일을 준 뒤 모양을 바꾼 글자도 그
//! 스타일로 남는다.

use crate::document_core::DocumentCore;
use crate::error::HwpError;
use crate::model::paragraph::Paragraph;

impl DocumentCore {
    fn char_style_paragraph(
        &mut self,
        sec: usize,
        para: usize,
        path: &[(usize, usize, usize)],
    ) -> Result<&mut Paragraph, HwpError> {
        if path.is_empty() {
            self.document
                .sections
                .get_mut(sec)
                .and_then(|s| s.paragraphs.get_mut(para))
                .ok_or_else(|| HwpError::RenderError(format!("문단 {sec}/{para} 범위 초과")))
        } else {
            self.get_cell_paragraph_mut_by_path(sec, para, path)
        }
    }

    /// 글자 `[start, end)` 에 글자 스타일 `style_id` 를 준다. `None` 이면 그 안에서 글자 스타일을
    /// 쓴 글자만 풀고 문단 스타일의 글자 모양으로 덮어 쓴다(한컴 "글자 스타일 해제", 다른 글은
    /// 그대로). `path` 가 비면 본문 문단, 아니면 셀·글상자 경로다. 바꾼 글자 수를 돌려준다.
    pub fn set_char_style_range_by_path(
        &mut self,
        sec: usize,
        para: usize,
        path: &[(usize, usize, usize)],
        start: usize,
        end: usize,
        style_id: Option<u32>,
    ) -> Result<String, HwpError> {
        let paragraph = self.char_style_paragraph(sec, para, path)?;
        let para_style = paragraph.style_id;
        // 바꿀 글자 구간: 줄 때는 범위 전체, 풀 때는 글자 스타일을 쓴 글자만.
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for i in start..end.min(paragraph.char_offsets.len()) {
            if style_id.is_none() && paragraph.char_style_id_at(i).is_none() {
                continue;
            }
            match runs.last_mut() {
                Some(run) if run.1 == i => run.1 = i + 1,
                _ => runs.push((i, i + 1)),
            }
        }
        let styles = &self.document.doc_info.styles;
        let shape = match style_id {
            Some(id) => {
                let style = styles
                    .get(id as usize)
                    .filter(|s| s.style_type == 1)
                    .ok_or_else(|| {
                        HwpError::RenderError(format!("{id}번은 글자 스타일이 아닙니다"))
                    })?;
                style.char_shape_id as u32
            }
            None => styles
                .get(para_style as usize)
                .map(|s| s.char_shape_id as u32)
                .ok_or_else(|| HwpError::RenderError("문단 스타일이 없습니다".into()))?,
        };
        for &(from, to) in &runs {
            if path.is_empty() {
                self.set_char_shape_id_native(sec, para, from, to, shape)?;
            } else {
                self.set_char_shape_id_in_cell_by_path(sec, para, path, from, to, shape)?;
            }
            self.char_style_paragraph(sec, para, path)?
                .set_char_style_range(from, to, style_id);
        }
        if !runs.is_empty() {
            self.document.sections[sec].raw_stream = None;
        }
        let changed: usize = runs.iter().map(|(from, to)| to - from).sum();
        Ok(format!("{{\"ok\":true,\"changed\":{changed}}}"))
    }

    /// 글자 `char_offset` 의 글자 스타일 번호. 없으면 -1.
    pub fn get_char_style_at_by_path(
        &mut self,
        sec: usize,
        para: usize,
        path: &[(usize, usize, usize)],
        char_offset: usize,
    ) -> Result<i64, HwpError> {
        Ok(self
            .char_style_paragraph(sec, para, path)?
            .char_style_id_at(char_offset)
            .map_or(-1, i64::from))
    }

    /// 글자 스타일 정의의 글자 모양이 `old` 에서 `new` 로 바뀌었을 때, 그 스타일을 쓴 글자를
    /// 문서 어디서나(본문·표·글상자·캡션·머리말·꼬리말·주석) 새 모양으로 바꾼다.
    pub(crate) fn replace_char_style_shapes(&mut self, style_id: u32, old: u32, new: u32) {
        for sec in 0..self.document.sections.len() {
            let mut body = Vec::new();
            let mut nested = false;
            for (pi, para) in self.document.sections[sec]
                .paragraphs
                .iter_mut()
                .enumerate()
            {
                if para.replace_char_style_shape(style_id, old, new) {
                    body.push(pi);
                }
                visit_nested_paragraphs(&mut para.controls, &mut |p| {
                    if p.replace_char_style_shape(style_id, old, new) {
                        // ponytail: 안쪽 문단은 줄을 다시 짜게 표시만 한다. 셀 높이가 어긋나면
                        // 셀 경로 리플로우(reflow_cell_paragraph_by_path)로 바꾼다.
                        p.invalidate_layout_inputs();
                        nested = true;
                    }
                });
            }
            for pi in &body {
                self.reflow_body_paragraph(sec, *pi);
            }
            if nested || !body.is_empty() {
                self.document.sections[sec].raw_stream = None;
            }
        }
    }
}

/// 컨트롤 안의 문단 목록(표 셀·캡션·글상자·묶음·머리말·꼬리말·각주·미주)을 깊이 우선으로 훑는다.
fn visit_nested_paragraphs(
    controls: &mut [crate::model::control::Control],
    f: &mut dyn FnMut(&mut Paragraph),
) {
    use crate::model::control::Control;
    use crate::model::shape::ShapeObject;
    fn list(paragraphs: &mut [Paragraph], f: &mut dyn FnMut(&mut Paragraph)) {
        for p in paragraphs {
            f(p);
            visit_nested_paragraphs(&mut p.controls, f);
        }
    }
    fn shape(s: &mut ShapeObject, f: &mut dyn FnMut(&mut Paragraph)) {
        if let ShapeObject::Group(group) = s {
            for child in &mut group.children {
                shape(child, f);
            }
        }
        if let Some(text_box) = s.drawing_mut().and_then(|d| d.text_box.as_mut()) {
            list(&mut text_box.paragraphs, f);
        }
    }
    for control in controls {
        match control {
            Control::Table(t) => {
                for cell in &mut t.cells {
                    list(&mut cell.paragraphs, f);
                }
                if let Some(c) = &mut t.caption {
                    list(&mut c.paragraphs, f);
                }
            }
            Control::Picture(pic) => {
                if let Some(c) = &mut pic.caption {
                    list(&mut c.paragraphs, f);
                }
            }
            Control::Shape(s) => shape(s, f),
            Control::Header(h) => list(&mut h.paragraphs, f),
            Control::Footer(h) => list(&mut h.paragraphs, f),
            Control::Footnote(n) => list(&mut n.paragraphs, f),
            Control::Endnote(n) => list(&mut n.paragraphs, f),
            _ => {}
        }
    }
}
