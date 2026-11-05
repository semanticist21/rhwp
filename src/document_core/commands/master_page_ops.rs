//! 바탕쪽 만들기·지우기·조회.
//!
//! 바탕쪽 문단의 글 편집은 머리말·꼬리말 편집 API가 `applyTo = MASTER_PAGE_APPLY_BASE + n`
//! 으로 맡는다(`header_footer_ops.rs`). 이 모듈은 바탕쪽 목록 자체와 모델 접근을 둔다.

use crate::document_core::{
    header_footer_apply_from_u8, header_footer_apply_to_u8, DocumentCore, MASTER_PAGE_APPLY_BASE,
};
use crate::error::HwpError;
use crate::model::header_footer::{HeaderFooterApply, MasterPage};
use crate::model::paragraph::Paragraph;
use crate::parser::tags::HWPTAG_LIST_HEADER;

/// 바탕쪽 하나의 조회 JSON 조각.
fn master_page_json(index: usize, master: &MasterPage) -> String {
    format!(
        "{{\"index\":{},\"applyTo\":{},\"isExtension\":{},\"overlap\":{},\"replaceBase\":{},\"front\":{},\"pageNumber\":{},\"paraCount\":{}}}",
        index,
        header_footer_apply_to_u8(master.apply_to),
        master.is_extension,
        // 확장 바탕쪽은 대신 그리지 않을 때만 겹친다(HWPX pageDuplicate).
        master.overlap && !(master.is_extension && master.replace_base),
        master.replace_base,
        master.page_front,
        master
            .optional_page_number
            .map_or("null".to_string(), |n| n.to_string()),
        master.paragraphs.len()
    )
}

impl DocumentCore {
    pub(crate) fn master_page_ref(
        &self,
        section_idx: usize,
        slot: usize,
    ) -> Result<&MasterPage, HwpError> {
        self.document
            .sections
            .get(section_idx)
            .and_then(|section| section.section_def.master_pages.get(slot))
            .ok_or_else(|| {
                HwpError::RenderError(format!(
                    "구역 {}의 {}번째 바탕쪽이 없습니다",
                    section_idx, slot
                ))
            })
    }

    /// 고칠 바탕쪽. HWP 원본 레코드가 가진 바탕쪽은 모델이 저장을 맡도록 소유를 넘긴다.
    pub(crate) fn master_page_mut(
        &mut self,
        section_idx: usize,
        slot: usize,
    ) -> Result<&mut MasterPage, HwpError> {
        self.master_page_ref(section_idx, slot)?;
        self.release_raw_master_pages(section_idx)?;
        Ok(&mut self.document.sections[section_idx].section_def.master_pages[slot])
    }

    /// HWP에서 읽은 바탕쪽은 원본 레코드를 그대로 다시 쓴다. 바탕쪽을 고치기 전에 그 레코드(구역 정의
    /// 자식의 기본 바탕쪽과 구역 끝 목록의 확장 바탕쪽)를 버려 모델에서 저장하게 한다. 구역 끝의 메모
    /// 목록은 그대로 둔다.
    fn release_raw_master_pages(&mut self, section_idx: usize) -> Result<(), HwpError> {
        let section = self
            .document
            .sections
            .get(section_idx)
            .ok_or_else(|| HwpError::RenderError(format!("구역 {} 범위 초과", section_idx)))?;
        let records = &section.section_def.extra_child_records;
        let top_level = records
            .iter()
            .filter(|raw| raw.tag_id == HWPTAG_LIST_HEADER)
            .map(|raw| raw.level)
            .min();
        let base_start = top_level.map(|top_level| {
            records
                .iter()
                .position(|raw| raw.tag_id == HWPTAG_LIST_HEADER && raw.level == top_level)
                .unwrap_or(records.len())
        });
        if let (Some(top_level), Some(start)) = (top_level, base_start) {
            // 영역이 0×0인 목록은 바탕쪽이 아니다(파서가 건너뛴 메모 목록). 함께 버리면 잃으므로 막는다.
            let all_masters = records[start..]
                .iter()
                .filter(|raw| raw.tag_id == HWPTAG_LIST_HEADER && raw.level == top_level)
                .all(|raw| raw.data.get(8..16).is_some_and(|area| area != [0u8; 8]));
            if !all_masters {
                return Err(HwpError::RenderError(
                    "바탕쪽이 아닌 원본 목록이 함께 저장된 구역입니다".to_string(),
                ));
            }
        }
        let tail_mask = section
            .trailing_lists
            .as_ref()
            .map(|tail| crate::model::section_tail::master_list_mask(&tail.records))
            .filter(|mask| mask.contains(&true));
        if base_start.is_none() && tail_mask.is_none() {
            return Ok(());
        }
        // 원본을 버리기 전에 구역 끝 목록의 봉인이 맞았으면, 버린 뒤의 투영으로 다시 봉인한다.
        // 이 구역의 바탕쪽은 이제 모델이 저장하므로 끝 목록과 어긋나지 않는다.
        let sealed = crate::model::section_tail::validate_preservation(&self.document).is_ok();
        let section = &mut self.document.sections[section_idx];
        if let Some(start) = base_start {
            section.section_def.extra_child_records.truncate(start);
        }
        if let Some(mask) = tail_mask {
            if let Some(tail) = section.trailing_lists.as_mut() {
                let mut flags = mask.into_iter();
                tail.records.retain(|_| !flags.next().unwrap_or(false));
                if tail.records.is_empty() {
                    section.trailing_lists = None;
                }
            }
        }
        section.raw_stream = None;
        if sealed {
            let digest = crate::model::section_tail::projection_digest(&self.document);
            for section in &mut self.document.sections {
                if let Some(tail) = section.trailing_lists.as_mut() {
                    tail.projection_seal = Some(digest);
                }
            }
        }
        Ok(())
    }

    /// 구역의 바탕쪽 목록. 반환: `{"ok":true,"masterPages":[{index,applyTo,isExtension,...}]}`
    pub fn get_master_pages_native(&self, section_idx: usize) -> Result<String, HwpError> {
        let section = self
            .document
            .sections
            .get(section_idx)
            .ok_or_else(|| HwpError::RenderError(format!("구역 {} 범위 초과", section_idx)))?;
        let items: Vec<String> = section
            .section_def
            .master_pages
            .iter()
            .enumerate()
            .map(|(index, master)| master_page_json(index, master))
            .collect();
        Ok(format!(
            "{{\"ok\":true,\"masterPages\":[{}]}}",
            items.join(",")
        ))
    }

    /// 쪽에서 고칠 바탕쪽. 그 쪽에 그린 바탕쪽이 있으면 그것을, 감춰 그리지 않았으면 그 쪽 구역(없으면
    /// 앞 구역)에서 쪽의 홀짝에 맞는 바탕쪽을 고른다. 한컴 도움말 "바탕쪽 고치기"처럼 현재 구역에
    /// 바탕쪽이 없으면 앞 구역으로 거슬러 찾는다.
    ///
    /// 반환: `{"ok":true,"sectionIndex":N,"applyTo":16+n}` 또는 `{"ok":true,"found":false}`
    pub fn get_master_page_edit_target_native(&self, page_num: u32) -> Result<String, HwpError> {
        let (page, _, _) = self.find_page(page_num)?;
        let target = page
            .active_master_page
            .as_ref()
            .map(|r| (r.section_index, r.master_page_index))
            .or_else(|| {
                let odd = page.page_number % 2 == 1;
                (0..=page.section_index).rev().find_map(|section_idx| {
                    let masters = &self.document.sections[section_idx].section_def.master_pages;
                    let base = |apply| {
                        masters
                            .iter()
                            .position(|m| !m.is_extension && m.apply_to == apply)
                    };
                    base(if odd {
                        HeaderFooterApply::Odd
                    } else {
                        HeaderFooterApply::Even
                    })
                    .or_else(|| base(HeaderFooterApply::Both))
                    .or_else(|| (!masters.is_empty()).then_some(0))
                    .map(|slot| (section_idx, slot))
                })
            });
        Ok(match target {
            Some((section_idx, slot)) => format!(
                "{{\"ok\":true,\"found\":true,\"sectionIndex\":{},\"applyTo\":{}}}",
                section_idx,
                usize::from(MASTER_PAGE_APPLY_BASE) + slot
            ),
            None => "{\"ok\":true,\"found\":false}".to_string(),
        })
    }

    /// 빈 문단 하나를 가진 바탕쪽을 만든다. 같은 종류(같은 기본 종류, 마지막 쪽, 같은 임의 쪽)가 있으면
    /// 오류다. 글 영역은 한컴처럼 구역 용지의 본문 영역이다(머리말·꼬리말 여백 제외).
    ///
    /// options:
    /// - `{"applyTo":0|1|2}`: 기본 바탕쪽(0 양 쪽, 1 짝수 쪽, 2 홀수 쪽)
    /// - `{"kind":"last"}`: 구역의 마지막 쪽 바탕쪽
    /// - `{"kind":"page","pageNumber":n}`: 구역 안 n째 쪽(1부터) 바탕쪽
    /// - 마지막 쪽·임의 쪽은 `"overlap":true`면 기본 바탕쪽 위에 겹쳐 그리고, 아니면 기본 바탕쪽 대신 그린다.
    /// - `"front":true`는 한컴 "인쇄할 때 바탕쪽 맨 위로 보내기"다.
    ///
    /// 반환: `{"ok":true,"applyTo":16+n}`
    pub fn create_master_page_native(
        &mut self,
        section_idx: usize,
        options_json: &str,
    ) -> Result<String, HwpError> {
        use crate::document_core::helpers::{json_bool, json_str, json_u16};
        let invalid = || HwpError::RenderError("바탕쪽 종류를 확인해 주세요".to_string());
        // 확장 바탕쪽의 지정 쪽: None은 마지막 쪽이다.
        let extension: Option<Option<u16>> = match json_str(options_json, "kind").as_deref() {
            None => None,
            Some("last") => Some(None),
            Some("page") => Some(Some(
                json_u16(options_json, "pageNumber")
                    .filter(|number| (1..u16::MAX - 3).contains(number))
                    .ok_or_else(invalid)?,
            )),
            Some(_) => return Err(invalid()),
        };
        let apply = match extension {
            Some(_) => HeaderFooterApply::Both,
            None => header_footer_apply_from_u8(
                json_u16(options_json, "applyTo")
                    .filter(|code| *code <= 2)
                    .ok_or_else(invalid)? as u8,
            ),
        };
        let overlap = extension.is_some() && json_bool(options_json, "overlap").unwrap_or(false);
        let section = self
            .document
            .sections
            .get(section_idx)
            .ok_or_else(|| HwpError::RenderError(format!("구역 {} 범위 초과", section_idx)))?;
        if section
            .section_def
            .master_pages
            .iter()
            .any(|m| match extension {
                None => !m.is_extension && m.apply_to == apply,
                Some(page) => m.is_extension && m.optional_page_number == page,
            })
        {
            return Err(HwpError::RenderError(
                "이미 같은 종류의 바탕쪽이 있습니다".to_string(),
            ));
        }
        let page = &section.section_def.page_def;
        let text_width = (page.width as i64
            - page.margin_left as i64
            - page.margin_right as i64
            - page.margin_gutter as i64)
            .max(0) as u32;
        let text_height = (page.height as i64
            - page.margin_top as i64
            - page.margin_bottom as i64
            - page.margin_header as i64
            - page.margin_footer as i64)
            .max(0) as u32;
        self.release_raw_master_pages(section_idx)?;
        let masters = &mut self.document.sections[section_idx].section_def.master_pages;
        // 한컴 12.30처럼 양 쪽·짝수 쪽·홀수 쪽, 구역 마지막 쪽, 구역 안 쪽 번호 차례로 둔다(이전·이후
        // 바탕쪽 차례이자 저장 차례). 확장 바탕쪽은 만든 차례와 관계없이 이 차례로 저장된다.
        let rank = |is_extension: bool, apply: HeaderFooterApply, page: Option<u16>| {
            if is_extension {
                3 + u32::from(page.unwrap_or(0))
            } else {
                match apply {
                    HeaderFooterApply::Both => 0,
                    HeaderFooterApply::Even => 1,
                    HeaderFooterApply::Odd => 2,
                }
            }
        };
        let new_rank = rank(extension.is_some(), apply, extension.flatten());
        let slot = masters
            .iter()
            .position(|m| rank(m.is_extension, m.apply_to, m.optional_page_number) > new_rank)
            .unwrap_or(masters.len());
        masters.insert(
            slot,
            MasterPage {
                apply_to: apply,
                is_extension: extension.is_some(),
                overlap,
                replace_base: extension.is_some() && !overlap,
                ext_flags: match extension {
                    None => 0,
                    Some(page) => 0x02 | u16::from(overlap) | if page.is_some() { 0x04 } else { 0 },
                },
                page_front: json_bool(options_json, "front").unwrap_or(false),
                optional_page_number: extension.flatten(),
                paragraphs: vec![Paragraph::default()],
                text_width,
                text_height,
                hwpx_page_number: Some(extension.flatten().unwrap_or(0)),
                ..Default::default()
            },
        );
        self.document.sections[section_idx].raw_stream = None;
        self.sync_section_def_controls_from_section(section_idx);
        self.mark_section_dirty(section_idx);
        self.paginate_if_needed();
        Ok(format!(
            "{{\"ok\":true,\"applyTo\":{}}}",
            usize::from(MASTER_PAGE_APPLY_BASE) + slot
        ))
    }

    /// 바탕쪽 하나를 지운다. `apply_to`는 `16 + n`이다.
    pub fn delete_master_page_native(
        &mut self,
        section_idx: usize,
        apply_to: u8,
    ) -> Result<String, HwpError> {
        let slot = crate::document_core::master_page_slot(apply_to)
            .ok_or_else(|| HwpError::RenderError("바탕쪽 번호를 확인해 주세요".to_string()))?;
        self.master_page_mut(section_idx, slot)?;
        self.document.sections[section_idx]
            .section_def
            .master_pages
            .remove(slot);
        self.document.sections[section_idx].raw_stream = None;
        self.sync_section_def_controls_from_section(section_idx);
        self.mark_section_dirty(section_idx);
        self.paginate_if_needed();
        Ok("{\"ok\":true}".to_string())
    }
}

#[cfg(test)]
mod tests {
    use crate::document_core::{DocumentCore, MASTER_PAGE_APPLY_BASE};
    use crate::parser::tags::HWPTAG_LIST_HEADER;

    fn blank() -> DocumentCore {
        let mut core = DocumentCore::new_empty();
        core.create_blank_document_native().unwrap();
        core
    }

    fn master_texts(core: &DocumentCore) -> Vec<String> {
        core.document.sections[0]
            .section_def
            .master_pages
            .iter()
            .map(|master| {
                master
                    .paragraphs
                    .iter()
                    .map(|p| p.text.clone())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .collect()
    }

    /// 바탕쪽 글이 그려진 첫 글자의 (x, y). SVG의 `<text x=".." y="..">글</text>`에서 찾는다.
    fn glyph_at(svg: &str, ch: char) -> Option<(f64, f64)> {
        let needle = format!(">{}</text>", ch);
        let end = svg.find(&needle)?;
        let tag = &svg[svg[..end].rfind("<text")?..end];
        let attr = |name: &str| -> Option<f64> {
            let start = tag.find(&format!(" {}=\"", name))? + name.len() + 3;
            tag[start..].split('"').next()?.parse().ok()
        };
        Some((attr("x")?, attr("y")?))
    }

    #[test]
    fn master_page_text_is_edited_through_header_footer_api_and_survives_both_formats() {
        let mut core = blank();
        let created = core
            .create_master_page_native(0, "{\"applyTo\":0}")
            .unwrap();
        assert!(created.contains("\"applyTo\":16"));
        let target = MASTER_PAGE_APPLY_BASE;
        assert!(core
            .create_master_page_native(0, "{\"applyTo\":0}")
            .is_err());

        core.insert_text_in_header_footer_native(0, false, target, 0, 0, "바탕글")
            .unwrap();
        core.split_paragraph_in_header_footer_native(0, false, target, 0, 2, None)
            .unwrap();
        assert_eq!(master_texts(&core), ["바탕/글"]);
        core.merge_paragraph_in_header_footer_native(0, false, target, 1)
            .unwrap();
        core.delete_text_in_header_footer_native(0, false, target, 0, 2, 1)
            .unwrap();
        core.insert_text_in_header_footer_native(0, false, target, 0, 2, "쪽")
            .unwrap();
        assert_eq!(master_texts(&core), ["바탕쪽"]);
        // 머리말·꼬리말은 생기지 않는다.
        assert!(core
            .get_header_footer_native(0, true, 0)
            .unwrap()
            .contains("\"exists\":false"));

        // 한컴처럼 바탕쪽 글은 본문 첫 글자 자리(본문 영역 왼쪽 위)에 그린다.
        let page = &core.document.sections[0].section_def.page_def;
        let px = |hu: u32| f64::from(hu) * 96.0 / 7200.0;
        let body_x = px(page.margin_left + page.margin_gutter);
        let body_y = px(page.margin_top + page.margin_header);
        let (x, y) = glyph_at(&core.render_page_svg_native(0).unwrap(), '바').unwrap();
        assert!((x - body_x).abs() < 1.0, "x={x} body_x={body_x}");
        assert!(y > body_y && y < body_y + 20.0, "y={y} body_y={body_y}");

        for bytes in [
            core.export_hwp_native().unwrap(),
            core.export_hwpx_native().unwrap(),
        ] {
            let reopened = DocumentCore::from_bytes(&bytes).unwrap();
            assert_eq!(master_texts(&reopened), ["바탕쪽"]);
            assert!(reopened
                .render_page_svg_native(0)
                .unwrap()
                .contains(">쪽</text>"));
        }
    }

    #[test]
    fn editing_a_master_page_read_from_hwp_saves_the_edit() {
        let mut core = blank();
        core.create_master_page_native(0, "{\"applyTo\":0}")
            .unwrap();
        core.insert_text_in_header_footer_native(0, false, MASTER_PAGE_APPLY_BASE, 0, 0, "처음")
            .unwrap();
        let mut reopened = DocumentCore::from_bytes(&core.export_hwp_native().unwrap()).unwrap();
        // HWP에서 읽은 바탕쪽은 원본 레코드를 지닌다.
        assert!(!reopened.document.sections[0]
            .section_def
            .extra_child_records
            .is_empty());
        reopened
            .insert_text_in_header_footer_native(0, false, MASTER_PAGE_APPLY_BASE, 0, 2, "고침")
            .unwrap();
        reopened
            .create_master_page_native(0, "{\"applyTo\":2}")
            .unwrap();
        let saved = DocumentCore::from_bytes(&reopened.export_hwp_native().unwrap()).unwrap();
        assert_eq!(master_texts(&saved), ["처음고침", ""]);
        // 한컴처럼 홀수 쪽 바탕쪽이 양 쪽 바탕쪽보다 앞서 1쪽(홀수)은 빈 홀수 쪽 바탕쪽을 쓴다.
        assert!(!saved
            .render_page_svg_native(0)
            .unwrap()
            .contains(">고</text>"));
    }

    #[test]
    fn base_and_tail_masters_of_a_hwp_with_a_section_tail_are_edited_and_saved() {
        // exam_math_8.hwp: 기본 바탕쪽은 구역 정의 자식, 임의 쪽 바탕쪽은 구역 끝 목록에 저장돼 있다.
        let bytes = std::fs::read("samples/exam_math_8.hwp").unwrap();
        let mut core = DocumentCore::from_bytes(&bytes).unwrap();
        let masters = &core.document.sections[0].section_def.master_pages;
        assert!(!masters[0].is_extension && masters[1].is_extension);
        let page = masters[1].optional_page_number;
        assert!(page.is_some());
        assert!(core.document.sections[0].trailing_lists.is_some());
        core.insert_text_in_header_footer_native(
            0,
            false,
            MASTER_PAGE_APPLY_BASE,
            0,
            0,
            "바탕고침",
        )
        .unwrap();
        core.insert_text_in_header_footer_native(0, false, MASTER_PAGE_APPLY_BASE + 1, 0, 0, "끝")
            .unwrap();
        let saved = DocumentCore::from_bytes(&core.export_hwp_native().unwrap()).unwrap();
        let masters = &saved.document.sections[0].section_def.master_pages;
        assert!(masters[0].paragraphs[0].text.starts_with("바탕고침"));
        // 끝 목록의 임의 쪽 바탕쪽도 고친 글로 같은 쪽에 남는다.
        assert!(masters[1].is_extension);
        assert_eq!(masters[1].optional_page_number, page);
        assert!(masters[1].paragraphs[0].text.starts_with('끝'));
        assert_eq!(masters.len(), 2);
    }

    /// 세 쪽짜리 빈 문서(쪽마다 "본문" 한 문단).
    fn three_pages() -> DocumentCore {
        let mut core = blank();
        core.insert_text_native(0, 0, 0, "본문").unwrap();
        for para in 0..2 {
            core.insert_page_break_native(0, para, 2).unwrap();
            core.insert_text_native(0, para + 1, 0, "본문").unwrap();
        }
        assert_eq!(core.page_count(), 3);
        core
    }

    fn set_master_text(core: &mut DocumentCore, slot: usize, text: &str) {
        core.insert_text_in_header_footer_native(
            0,
            false,
            MASTER_PAGE_APPLY_BASE + slot as u8,
            0,
            0,
            text,
        )
        .unwrap();
    }

    /// 쪽에 그린 바탕쪽 글(바탕쪽마다 서로 다른 첫 글자로 찾는다).
    fn drawn(core: &DocumentCore, page: u32, marks: &[char]) -> String {
        let svg = core.render_page_svg_native(page).unwrap();
        marks
            .iter()
            .filter(|mark| svg.contains(&format!(">{}</text>", mark)))
            .collect()
    }

    /// 바탕쪽 LIST_HEADER의 종류 값(byte 18-19)과 속성(byte 22-25) — 저장 차례대로.
    fn hwp_master_lists(bytes: &[u8]) -> Vec<(u16, u32)> {
        let core = DocumentCore::from_bytes(bytes).unwrap();
        let section = &core.document.sections[0];
        let mut raws: Vec<&[u8]> = section
            .section_def
            .extra_child_records
            .iter()
            .chain(
                section
                    .trailing_lists
                    .iter()
                    .flat_map(|tail| tail.records.iter()),
            )
            .filter(|raw| raw.tag_id == HWPTAG_LIST_HEADER && raw.data.len() >= 26)
            .map(|raw| raw.data.as_slice())
            .collect();
        raws.retain(|data| data[8..16] != [0u8; 8]);
        raws.iter()
            .map(|data| {
                (
                    u16::from_le_bytes([data[18], data[19]]),
                    u32::from_le_bytes([data[22], data[23], data[24], data[25]]),
                )
            })
            .collect()
    }

    #[test]
    fn last_and_optional_page_masters_follow_hancom_storage_and_drawing() {
        // 한컴 12.30 저장본 e-ext·b-ext·d-ext·c-front와 같은 순서로 만든다: 임의 2쪽(겹치게), 양 쪽,
        // 마지막 쪽, 임의 3쪽.
        let mut core = three_pages();
        core.create_master_page_native(0, r#"{"kind":"page","pageNumber":2,"overlap":true}"#)
            .unwrap();
        core.create_master_page_native(0, r#"{"applyTo":0}"#)
            .unwrap();
        core.create_master_page_native(0, r#"{"kind":"last"}"#)
            .unwrap();
        core.create_master_page_native(0, r#"{"kind":"page","pageNumber":3,"front":true}"#)
            .unwrap();
        // 같은 마지막 쪽·같은 임의 쪽은 둘 만들지 않는다.
        assert!(core
            .create_master_page_native(0, r#"{"kind":"last"}"#)
            .is_err());
        assert!(core
            .create_master_page_native(0, r#"{"kind":"page","pageNumber":2}"#)
            .is_err());
        assert!(core
            .create_master_page_native(0, r#"{"kind":"page","pageNumber":0}"#)
            .is_err());
        // 저장·이동 차례: 양 쪽, 마지막 쪽, 임의 쪽 번호 차례(한컴 12.30).
        let json = core.get_master_pages_native(0).unwrap();
        let order: Vec<_> = core.document.sections[0]
            .section_def
            .master_pages
            .iter()
            .map(|m| (m.is_extension, m.optional_page_number))
            .collect();
        assert_eq!(
            order,
            [
                (false, None),
                (true, None),
                (true, Some(2)),
                (true, Some(3))
            ],
            "{json}"
        );
        for (slot, text) in ["가양", "나끝", "다둘", "라셋"].iter().enumerate() {
            set_master_text(&mut core, slot, text);
        }
        let marks = ['가', '나', '다', '라'];
        // 1쪽 양 쪽, 2쪽 양 쪽 위에 임의 2쪽(겹치게 하기), 3쪽은 마지막 쪽 바탕쪽만(같은 쪽 임의 3쪽은 그리지 않음).
        let expect = |core: &DocumentCore| {
            assert_eq!(drawn(core, 0, &marks), "가");
            assert_eq!(drawn(core, 1, &marks), "가다");
            assert_eq!(drawn(core, 2, &marks), "나");
        };
        expect(&core);

        let hwp = core.export_hwp_native().unwrap();
        // 한컴 12.30 HWP: 기본 바탕쪽은 구역 정의 아래 종류 0, 확장 바탕쪽은 구역 끝 목록에 종류 3(마지막 쪽)·
        // n + 3(임의 쪽)과 속성 bit 0 겹치게 하기·bit 1 맨 위로 보내기.
        assert_eq!(hwp_master_lists(&hwp), [(0, 0), (3, 0), (5, 1), (6, 2)]);
        let hwpx = core.export_hwpx_native().unwrap();
        for bytes in [&hwp, &hwpx] {
            let reopened = DocumentCore::from_bytes(bytes).unwrap();
            expect(&reopened);
            let masters = &reopened.document.sections[0].section_def.master_pages;
            assert_eq!(
                masters
                    .iter()
                    .map(|m| (m.optional_page_number, m.replace_base, m.page_front))
                    .collect::<Vec<_>>(),
                [
                    (None, false, false),
                    (None, true, false),
                    (Some(2), false, false),
                    (Some(3), true, true)
                ]
            );
        }
        // HWPX에서 연 바탕쪽을 HWP로 저장해도(앱 저장 경로) 같은 종류·속성이다.
        let via_hwpx = DocumentCore::from_bytes(&hwpx).unwrap();
        assert_eq!(
            hwp_master_lists(&via_hwpx.export_hwp_with_adapter_snapshot().unwrap()),
            [(0, 0), (3, 0), (5, 1), (6, 2)]
        );
    }

    #[test]
    fn front_master_is_drawn_above_the_body() {
        let mut core = three_pages();
        core.create_master_page_native(0, r#"{"applyTo":0,"front":true}"#)
            .unwrap();
        set_master_text(&mut core, 0, "앞");
        let svg = core.render_page_svg_native(0).unwrap();
        let master = svg.find(">앞</text>").unwrap();
        let body = svg.find(">본</text>").unwrap();
        assert!(
            master > body,
            "맨 위로 보내기 바탕쪽은 본문 뒤에 그린다(위에 놓인다)"
        );
        let mut back = three_pages();
        back.create_master_page_native(0, r#"{"applyTo":0}"#)
            .unwrap();
        set_master_text(&mut back, 0, "뒤");
        let svg = back.render_page_svg_native(0).unwrap();
        assert!(svg.find(">뒤</text>").unwrap() < svg.find(">본</text>").unwrap());
    }

    #[test]
    fn hwp_stores_master_kinds_in_section_flags_in_hancom_order() {
        use crate::model::header_footer::HeaderFooterApply::{Both, Even, Odd};
        // 한컴 12.30 저장본: 양 쪽·짝수 쪽·홀수 쪽 차례의 목록과 구역 정의 bit 29·30·31.
        for (kinds, flags) in [
            (vec![0u8], 0x2000_0000u32),
            (vec![1], 0x4000_0000),
            (vec![2], 0x8000_0000),
            (vec![2, 0], 0xa000_0000),
            (vec![2, 1], 0xc000_0000),
            (vec![2, 1, 0], 0xe000_0000),
        ] {
            let mut core = blank();
            for kind in &kinds {
                core.create_master_page_native(0, &format!("{{\"applyTo\":{kind}}}"))
                    .unwrap();
            }
            for slot in 0..kinds.len() {
                core.insert_text_in_header_footer_native(
                    0,
                    false,
                    MASTER_PAGE_APPLY_BASE + slot as u8,
                    0,
                    0,
                    &format!(
                        "{:?}",
                        core.document.sections[0].section_def.master_pages[slot].apply_to
                    ),
                )
                .unwrap();
            }
            let order: Vec<_> = core.document.sections[0]
                .section_def
                .master_pages
                .iter()
                .map(|m| m.apply_to)
                .collect();
            let expected: Vec<_> = [Both, Even, Odd]
                .into_iter()
                .filter(|kind| order.contains(kind))
                .collect();
            assert_eq!(order, expected, "만든 차례와 관계없이 한컴 차례로 둔다");
            let saved = DocumentCore::from_bytes(&core.export_hwp_native().unwrap()).unwrap();
            let section = &saved.document.sections[0];
            assert_eq!(section.section_def.flags & 0xe000_0000, flags);
            for master in &section.section_def.master_pages {
                assert_eq!(master.paragraphs[0].text, format!("{:?}", master.apply_to));
            }
            assert_eq!(section.section_def.master_pages.len(), kinds.len());
        }
    }

    #[test]
    fn master_page_edit_view_draws_only_that_master_page_and_answers_caret_queries() {
        let mut core = blank();
        core.insert_text_native(0, 0, 0, "본문").unwrap();
        core.create_master_page_native(0, "{\"applyTo\":0}")
            .unwrap();
        let target = MASTER_PAGE_APPLY_BASE;
        core.insert_text_in_header_footer_native(0, false, target, 0, 0, "바탕")
            .unwrap();

        let page = core.render_page_svg_native(0).unwrap();
        assert!(page.contains(">본</text>") && page.contains(">바</text>"));
        let tree = core
            .build_header_footer_edit_preview_tree(0, 0, false, target)
            .unwrap();
        let mut text = String::new();
        fn collect(node: &crate::renderer::render_tree::RenderNode, out: &mut String) {
            if let crate::renderer::render_tree::RenderNodeType::TextRun(run) = &node.node_type {
                out.push_str(&run.text);
            }
            node.children.iter().for_each(|child| collect(child, out));
        }
        collect(&tree.root, &mut text);
        assert_eq!(text, "바탕");

        let rect = core
            .get_cursor_rect_in_header_footer_native(0, false, target, 0, 1, 0)
            .unwrap();
        let json: serde_json::Value = serde_json::from_str(&rect).unwrap();
        let (x, y) = (json["x"].as_f64().unwrap(), json["y"].as_f64().unwrap());
        let hit: serde_json::Value = serde_json::from_str(
            &core
                .hit_test_in_header_footer_target_native(0, 0, false, target, x + 0.5, y + 4.0)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(hit["hit"], true);
        assert_eq!(hit["paraIndex"], 0);
        assert_eq!(hit["charOffset"], 1);
        let rects = core
            .get_selection_rects_in_header_footer_native(0, false, target, 0, 0, 0, 0, 2)
            .unwrap();
        assert!(rects.contains("\"width\""), "{rects}");

        let edit_target = core.get_master_page_edit_target_native(0).unwrap();
        assert!(edit_target.contains("\"sectionIndex\":0,\"applyTo\":16"));
        core.delete_master_page_native(0, target).unwrap();
        assert!(master_texts(&core).is_empty());
        assert!(!core
            .render_page_svg_native(0)
            .unwrap()
            .contains(">바</text>"));
        assert!(core
            .get_master_page_edit_target_native(0)
            .unwrap()
            .contains("\"found\":false"));
    }
}
