//! 글자 스타일 참조 — 한컴은 글자 스타일을 글자에 따로 적는다.
//!
//! HWP5는 `PARA_RANGE_TAG` 종류 1(하위 24비트 = 스타일 번호, 끝 exclusive)로, HWPX는
//! `<hp:t charStyleIDRef>`로 싣는다. 한컴 12.30 Mac 저장본 실측: `가[나다라]마바`에
//! 글자 스타일 12를 주면 `(1, 4, 0x0100000C)`와 문단 전체 `(0, 6, 0)`을 함께 쓴다.
//! 종류 0 태그 없이 종류 1만 쓴 HWP도 한컴이 연다(HWPX→HWP 변환의 `0x0100_0023`).
//! 모델은 HWP 표기를 그대로 쓴다 — 글 넣기·지우기·나누기·합치기가 영역 태그를 이미
//! 옮기므로 글자 스타일도 같은 길을 탄다.

use super::{Paragraph, RangeTag};

/// 글자 스타일 영역 태그의 종류(상위 8비트).
pub(crate) const CHAR_STYLE_TAG_KIND: u32 = 1;

impl RangeTag {
    /// 글자 스타일 영역이면 그 스타일 번호.
    pub fn char_style_id(&self) -> Option<u32> {
        (self.tag >> 24 == CHAR_STYLE_TAG_KIND).then_some(self.tag & 0x00ff_ffff)
    }
}

impl Paragraph {
    /// 글자 위치(`text` 의 문자 인덱스)의 UTF-16 스트림 위치. 문단 끝이면 마지막 글자 뒤.
    fn char_style_stream_pos(&self, char_idx: usize) -> u32 {
        if let Some(&pos) = self.char_offsets.get(char_idx) {
            return pos;
        }
        match (self.char_offsets.last(), self.text.chars().last()) {
            (Some(&last), Some(ch)) => last + ch.len_utf16() as u32,
            _ => 0,
        }
    }

    /// 글자 `char_idx` 에 걸린 글자 스타일 번호.
    pub fn char_style_id_at(&self, char_idx: usize) -> Option<u32> {
        if char_idx >= self.char_offsets.len() {
            return None;
        }
        let pos = self.char_style_stream_pos(char_idx);
        self.range_tags
            .iter()
            .filter(|t| t.start <= pos && pos < t.end)
            .find_map(RangeTag::char_style_id)
    }

    /// 글자 `[start, end)` 의 글자 스타일을 `style_id` 로 바꾸거나(`None` 이면 지운다).
    /// 글자 모양은 건드리지 않는다.
    pub fn set_char_style_range(&mut self, start: usize, end: usize, style_id: Option<u32>) {
        if start >= end {
            return;
        }
        let (s, e) = (
            self.char_style_stream_pos(start),
            self.char_style_stream_pos(end),
        );
        let mut tags = Vec::with_capacity(self.range_tags.len() + 2);
        for tag in self.range_tags.drain(..) {
            if tag.char_style_id().is_none() || tag.end <= s || tag.start >= e {
                tags.push(tag);
                continue;
            }
            // 바꾸는 범위 밖에 남는 앞뒤 조각만 둔다.
            if tag.start < s {
                tags.push(RangeTag {
                    end: s,
                    ..tag.clone()
                });
            }
            if tag.end > e {
                tags.push(RangeTag { start: e, ..tag });
            }
        }
        if let Some(id) = style_id {
            tags.push(RangeTag {
                start: s,
                end: e,
                tag: (CHAR_STYLE_TAG_KIND << 24) | (id & 0x00ff_ffff),
            });
        }
        self.range_tags = tags;
        self.normalize_char_style_tags();
    }

    /// 빈 글자 스타일 영역을 버리고 맞닿은 같은 스타일 영역을 하나로 잇는다.
    pub(crate) fn normalize_char_style_tags(&mut self) {
        self.range_tags
            .retain(|t| t.char_style_id().is_none() || t.start < t.end);
        self.range_tags.sort_by_key(|t| (t.start, t.end));
        let mut merged: Vec<RangeTag> = Vec::with_capacity(self.range_tags.len());
        for tag in self.range_tags.drain(..) {
            if let Some(last) = merged
                .iter_mut()
                .rev()
                .find(|t| t.char_style_id().is_some())
                .filter(|t| t.tag == tag.tag && t.end >= tag.start)
            {
                if tag.char_style_id().is_some() {
                    last.end = last.end.max(tag.end);
                    continue;
                }
            }
            merged.push(tag);
        }
        self.range_tags = merged;
    }

    /// 글자 스타일 `style_id` 를 쓴 글자 가운데 글자 모양이 `old` 인 글자만 `new` 로 바꾼다.
    /// 스타일을 준 뒤 직접 바꾼 모양과 같은 모양 번호의 보통 글은 그대로 둔다.
    pub fn replace_char_style_shape(&mut self, style_id: u32, old: u32, new: u32) -> bool {
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for i in 0..self.char_offsets.len() {
            if self.char_style_id_at(i) != Some(style_id) || self.char_shape_id_at(i) != Some(old) {
                continue;
            }
            match runs.last_mut() {
                Some(run) if run.1 == i => run.1 = i + 1,
                _ => runs.push((i, i + 1)),
            }
        }
        for &(start, end) in &runs {
            self.apply_char_shape_range(start, end, new);
        }
        !runs.is_empty()
    }
}
