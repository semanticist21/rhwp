//! HWP5 구역 끝 목록의 저장 소유. 적용/렌더링 플래그와 저장 위치는 별개다.

use super::control::{Control, Field, FieldType};
use super::document::{Document, RawRecord};
use super::paragraph::Paragraph;
use super::shape::{Caption, ShapeObject};

/// 미해석 정보까지 포함하는 원본 목록. 본문 편집으로 이 범위를 버리지 않는다.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct SectionTail {
    pub records: Vec<RawRecord>,
    /// 로드 후 모델 정규화가 끝난 시점의 바탕쪽·메모 참조 투영.
    #[serde(skip)]
    pub(crate) projection_seal: Option<[u8; 32]>,
}

enum Node<'a> {
    Paragraphs(&'a [Paragraph]),
    Shape(&'a ShapeObject),
}

fn caption<'a>(stack: &mut Vec<Node<'a>>, caption: Option<&'a Caption>) {
    if let Some(caption) = caption {
        stack.push(Node::Paragraphs(&caption.paragraphs));
    }
}

/// 구역 끝 원본 목록에서 바탕쪽 목록(메모 목록 바로 뒤가 아닌 수준 1 LIST_HEADER와 그 문단)의
/// 레코드인지를 차례로 표시한다. 메모 목록은 MEMO_LIST 다음 LIST_HEADER로 시작한다.
pub(crate) fn master_list_mask(records: &[RawRecord]) -> Vec<bool> {
    use crate::parser::tags::{HWPTAG_LIST_HEADER, HWPTAG_MEMO_LIST};
    let mut master = false;
    let mut after_memo = false;
    records
        .iter()
        .map(|raw| {
            if raw.level == 1 {
                if raw.tag_id == HWPTAG_LIST_HEADER {
                    master = !after_memo;
                } else if raw.tag_id == HWPTAG_MEMO_LIST {
                    master = false;
                }
                after_memo = raw.tag_id == HWPTAG_MEMO_LIST;
            }
            master
        })
        .collect()
}

/// 구역 끝 원본 목록이 바탕쪽 목록을 가졌는지. 가졌으면 그 구역의 확장 바탕쪽은 원본이 저장한다.
pub(crate) fn holds_master_lists(tail: &SectionTail) -> bool {
    master_list_mask(&tail.records).contains(&true)
}

/// 본문 글자/캐럿 위치는 제외하고, 원본 목록의 의미·참조를 바꾸는 모델만 봉인한다.
/// 메모는 마지막 구역에 저장돼 다른 구역의 anchor를 참조할 수 있으므로 문서 단위다.
pub(crate) fn projection_digest(doc: &Document) -> [u8; 32] {
    // 확장 바탕쪽(마지막 쪽·임의 쪽)은 구역 끝 원본 목록이, 기본 바탕쪽은 구역 정의 자식의 원본
    // 레코드가 남아 있는 동안만 원본으로 저장되므로 그때만 봉인한다. 바탕쪽 편집 API가 원본 레코드를
    // 버리면 모델이 저장을 맡는다.
    let masters: Vec<Vec<_>> = doc
        .sections
        .iter()
        .map(|s| {
            let raw_base = s
                .section_def
                .extra_child_records
                .iter()
                .any(|raw| raw.tag_id == crate::parser::tags::HWPTAG_LIST_HEADER);
            let raw_tail = s.trailing_lists.as_ref().is_some_and(holds_master_lists);
            s.section_def
                .master_pages
                .iter()
                .filter(|master| raw_base || (master.is_extension && raw_tail))
                .collect()
        })
        .collect();
    let mut memos: Vec<&Field> = Vec::new();
    let mut stack: Vec<_> = doc
        .sections
        .iter()
        .rev()
        .map(|s| Node::Paragraphs(&s.paragraphs))
        .collect();
    while let Some(node) = stack.pop() {
        match node {
            Node::Paragraphs(paragraphs) => {
                for paragraph in paragraphs {
                    for control in &paragraph.controls {
                        match control {
                            Control::Table(table) => {
                                for cell in &table.cells {
                                    stack.push(Node::Paragraphs(&cell.paragraphs));
                                }
                                caption(&mut stack, table.caption.as_ref());
                            }
                            Control::Shape(shape) => stack.push(Node::Shape(shape)),
                            Control::Picture(picture) => {
                                caption(&mut stack, picture.caption.as_ref())
                            }
                            Control::Header(h) => stack.push(Node::Paragraphs(&h.paragraphs)),
                            Control::Footer(h) => stack.push(Node::Paragraphs(&h.paragraphs)),
                            Control::Footnote(n) => stack.push(Node::Paragraphs(&n.paragraphs)),
                            Control::Endnote(n) => stack.push(Node::Paragraphs(&n.paragraphs)),
                            Control::HiddenComment(c) => {
                                stack.push(Node::Paragraphs(&c.paragraphs))
                            }
                            Control::Field(field) => {
                                if field.field_type == FieldType::Memo
                                    || field.command.starts_with("MEMO/")
                                {
                                    memos.push(field);
                                }
                                stack.push(Node::Paragraphs(&field.memo_paragraphs));
                            }
                            Control::SectionDef(sd) => {
                                for master in &sd.master_pages {
                                    stack.push(Node::Paragraphs(&master.paragraphs));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Node::Shape(shape) => {
                if let Some(drawing) = shape.drawing() {
                    if let Some(text_box) = &drawing.text_box {
                        stack.push(Node::Paragraphs(&text_box.paragraphs));
                    }
                    caption(&mut stack, drawing.caption.as_ref());
                }
                match shape {
                    ShapeObject::Group(group) => {
                        caption(&mut stack, group.caption.as_ref());
                        stack.extend(group.children.iter().map(Node::Shape));
                    }
                    ShapeObject::Picture(picture) => caption(&mut stack, picture.caption.as_ref()),
                    _ => {}
                }
            }
        }
    }
    // 메모 목록을 원본 꼬리가 아니라 메모 필드가 가지면(파서가 붙인 HWP 메모·새 메모) 메모를 고쳐도
    // 원본 꼬리와 어긋나지 않는다. 원본 꼬리에 메모 목록이 남은 때만 메모를 함께 봉인한다.
    let raw_memos = doc.sections.iter().any(|section| {
        section.trailing_lists.as_ref().is_some_and(|tail| {
            tail.records
                .iter()
                .any(|raw| raw.level == 1 && raw.tag_id == crate::parser::tags::HWPTAG_MEMO_LIST)
        })
    });
    if !raw_memos {
        memos.clear();
    }
    crate::model::raw_provenance::value_digest(
        &(masters, memos),
        "구역 끝 모델 투영은 메모리에서 직렬화한다",
    )
}

pub(crate) fn validate_preservation(doc: &Document) -> Result<(), String> {
    if !doc.sections.iter().any(|s| s.trailing_lists.is_some()) {
        return Ok(());
    }
    let digest = projection_digest(doc);
    for (index, section) in doc.sections.iter().enumerate() {
        if let Some(tail) = &section.trailing_lists {
            if tail.projection_seal != Some(digest) {
                return Err(format!("구역 {index}의 원본 바탕쪽·메모 목록과 현재 모델의 참조가 달라졌습니다. 이 목록 자체의 재작성은 아직 지원하지 않습니다."));
            }
        }
    }
    Ok(())
}
