//! 차트 넣기 — 한컴 12.30 `입력 > 차트 > 차트 만들기`·`표 > 차트 만들기`가 만드는 개체.
//!
//! 한컴은 캐럿 문단에 떠 있는 차트(32250×18750, 문단·단 기준 0, 어울림·양쪽)를 넣는다.
//! 본문·표 칸·각주에서 같은 개체다. 개체의 OLE 저장소는 한컴이 만든 그대로 담는다 —
//! `Contents`(레거시 VtChart), 빈 `\x02OlePres000`, `OOXMLChartContents` 세 스트림과 루트 CLSID.
//! 한컴은 차트를 열 때 OOXML 쪽 값을 그린다. 그래서 데이터는 OOXML 에만 쓰고(값 편집
//! `setChartData` 와 같다) 레거시 스트림은 종류별 한컴 원본을 둔다.
//!
//! 문서 형식마다 한컴 저장본과 같은 모델을 만든다.
//! - HWP: 일반 OLE 개체 + `BinData/BINxxxx.OLE` 저장소 하나.
//! - HWPX: `<hp:switch>` 의 `<hp:chart chartIDRef="Chart/chartN.xml">` + `<hp:default><hp:ole>`.
//!   모델로는 바깥 차트 OLE(60000+N) + 대체 OLE 이고, `Chart/chartN.xml` 과 `BinData` OLE 가
//!   같은 OOXML 을 갖는다. HWP 로 저장하면 `fold_hwpx_chart_ole_for_hwp` 가 대체 OLE 로 접는다.

use std::io::Read;

use crate::document_core::queries::chart_extract::collect_charts;
use crate::document_core::DocumentCore;
use crate::error::HwpError;
use crate::model::bin_data::{
    BinData, BinDataBytes, BinDataCompression, BinDataContent, BinDataStatus, BinDataType,
};
use crate::model::control::Control;
use crate::model::event::DocumentEvent;
use crate::model::shape::{
    CommonObjAttr, DrawingObjAttr, HorzAlign, HorzRelTo, ObjectNumberingType, OleDrawingAspect,
    OleShape, ShapeComponentAttr, ShapeObject, TextFlow, TextWrap, VertAlign, VertRelTo,
};
use crate::parser::ole_container::parse_ole_container;
use crate::parser::FileFormat;
use crate::serializer::ole_container::replace_ole_stream;

use super::chart::patched_chart_xml;

/// 한컴 12.30 차트 만들기의 개체 크기(HWPUNIT, 11.38cm × 6.61cm).
const DEFAULT_WIDTH: u32 = 32250;
const DEFAULT_HEIGHT: u32 = 18750;
/// HWPX 차트 파트의 `bin_data_content` id 기준값(`Chart/chartN.xml` = 60000 + N).
const CHART_ID_BASE: u16 = 60000;
const OOXML_STREAM: &str = "OOXMLChartContents";

/// 종류별 한컴 원본 OLE 저장소(CFB, raw deflate). 한컴 12.30 갤러리의 각 묶음 첫 차트다.
const TEMPLATES: &[(&str, &[u8])] = &[
    // 가로 막대형(묶은 가로 막대) — 차트 만들기의 기본
    ("bar", include_bytes!("chart_templates/bar.ole.deflate")),
    // 세로 막대형(묶은 세로 막대)
    ("column", include_bytes!("chart_templates/col.ole.deflate")),
    // 꺾은선형
    ("line", include_bytes!("chart_templates/line.ole.deflate")),
    // 영역형
    ("area", include_bytes!("chart_templates/area.ole.deflate")),
    // 원형
    ("pie", include_bytes!("chart_templates/pie.ole.deflate")),
    // 분산형(표식만)
    (
        "scatter",
        include_bytes!("chart_templates/scatter.ole.deflate"),
    ),
    // 방사형
    ("radar", include_bytes!("chart_templates/radar.ole.deflate")),
];

fn template_cfb(kind: &str) -> Result<Vec<u8>, HwpError> {
    let kind = if kind.is_empty() { "bar" } else { kind };
    let packed = TEMPLATES
        .iter()
        .find(|(name, _)| *name == kind)
        .map(|(_, bytes)| *bytes)
        .ok_or_else(|| HwpError::RenderError(format!("차트 종류 '{kind}' 를 넣을 수 없습니다")))?;
    let mut out = Vec::new();
    flate2::read::DeflateDecoder::new(packed)
        .read_to_end(&mut out)
        .map_err(|e| HwpError::RenderError(format!("차트 원본을 풀지 못했습니다: {e}")))?;
    Ok(out)
}

/// `options_json` 의 `width`·`height`(HWPUNIT). 없으면 한컴 기본 크기다.
fn chart_size(options_json: &str) -> Result<(u32, u32), HwpError> {
    if options_json.trim().is_empty() {
        return Ok((DEFAULT_WIDTH, DEFAULT_HEIGHT));
    }
    let value: serde_json::Value = serde_json::from_str(options_json)
        .map_err(|e| HwpError::RenderError(format!("차트 옵션 JSON 을 읽을 수 없습니다: {e}")))?;
    let size = |key: &str, default: u32| -> Result<u32, HwpError> {
        match value.get(key) {
            None | Some(serde_json::Value::Null) => Ok(default),
            Some(v) => v
                .as_u64()
                .filter(|n| (super::MIN_SHAPE_SIZE as u64..=0x7fff_ffff).contains(n))
                .map(|n| n as u32)
                .ok_or_else(|| HwpError::RenderError(format!("차트 {key} 를 확인해 주세요"))),
        }
    };
    Ok((
        size("width", DEFAULT_WIDTH)?,
        size("height", DEFAULT_HEIGHT)?,
    ))
}

/// 데이터를 적용한 OLE 저장소(CFB)와 그 OOXML. 데이터 거부는 `Err(Ok(부정 봉투))`.
fn chart_payload(
    kind: &str,
    data_json: &str,
) -> Result<(Vec<u8>, Vec<u8>), Result<String, HwpError>> {
    let cfb = template_cfb(kind).map_err(Err)?;
    let xml = parse_ole_container(&cfb)
        .and_then(|c| c.ooxml_chart)
        .ok_or_else(|| {
            Err(HwpError::RenderError(
                "차트 원본에 OOXML 이 없습니다".to_string(),
            ))
        })?;
    if data_json.trim().is_empty() {
        return Ok((cfb, xml));
    }
    let patched = patched_chart_xml(&xml, data_json).map_err(Ok)?;
    let cfb = replace_ole_stream(&cfb, OOXML_STREAM, &patched)
        .map_err(|e| Err(HwpError::RenderError(e.to_string())))?;
    Ok((cfb, patched))
}

impl DocumentCore {
    /// 본문 문단의 글자 자리에 한컴 차트 만들기와 같은 차트를 넣는다.
    ///
    /// `chart_type`: `bar`(기본)·`column`·`line`·`area`·`pie`·`scatter`·`radar`.
    /// `data_json`: 비우면 한컴 기본 데이터, 주면 `setChartData` 와 같은 편집 JSON
    /// (`{"structure":true,"labels":[…],"series":[{"name":…,"values":[…]}]}`).
    /// `options_json`: `{"width":HWPUNIT,"height":HWPUNIT}` (생략 가능).
    /// 반환: `{"ok":true,"paraIdx":N,"controlIdx":N,"chartIndex":N}`. 데이터를 받을 수 없으면
    /// 문서를 바꾸지 않고 `{"ok":false,"invalid":[…]}` 다.
    pub fn insert_chart_native(
        &mut self,
        section_idx: usize,
        para_idx: usize,
        char_offset: usize,
        chart_type: &str,
        data_json: &str,
        options_json: &str,
    ) -> Result<String, HwpError> {
        let section =
            self.document.sections.get(section_idx).ok_or_else(|| {
                HwpError::RenderError(format!("구역 인덱스 {section_idx} 범위 초과"))
            })?;
        if para_idx >= section.paragraphs.len() {
            return Err(HwpError::RenderError(format!(
                "문단 인덱스 {para_idx} 범위 초과"
            )));
        }
        let size = chart_size(options_json)?;
        let (cfb, xml) = match chart_payload(chart_type, data_json) {
            Ok(v) => v,
            Err(refusal) => return refusal,
        };
        let control = self.register_chart(section_idx, size, cfb, xml);

        self.document.sections[section_idx].raw_stream = None;
        let paragraph = &mut self.document.sections[section_idx].paragraphs[para_idx];
        let insert_idx = Self::insert_inline_control(paragraph, char_offset, control);
        self.shift_active_field_for_control_insert(section_idx, para_idx, insert_idx);
        self.reflow_paragraph(section_idx, para_idx);
        self.recompose_section(section_idx);
        self.paginate_if_needed();
        self.invalidate_page_tree_cache();
        self.event_log.push(DocumentEvent::PictureInserted {
            section: section_idx,
            para: para_idx,
        });

        let chart_index = collect_charts(&self.document)
            .iter()
            .position(|c| {
                c.is_top_level()
                    && c.section == section_idx
                    && c.paragraph == para_idx
                    && c.control == insert_idx
            })
            .unwrap_or_default();
        Ok(format!(
            "{{\"ok\":true,\"paraIdx\":{para_idx},\"controlIdx\":{insert_idx},\"chartIndex\":{chart_index}}}"
        ))
    }

    /// 셀 경로가 가리키는 문단(표 칸·글상자·각주·미주·머리말 안 문단)의 글자 자리에 차트를 넣는다.
    /// 한컴 12.30은 표 칸과 각주 안에서도 본문과 같은 떠 있는 차트를 넣는다.
    /// 인자는 `insert_chart_native` 와 같고 `path` 는 다른 by-path API 의 셀 경로다.
    /// 반환: `{"ok":true,"cellParaIdx":N,"controlIdx":N,"chartIndex":N}`.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_chart_by_path_native(
        &mut self,
        section_idx: usize,
        parent_para_idx: usize,
        path: &[(usize, usize, usize)],
        char_offset: usize,
        chart_type: &str,
        data_json: &str,
        options_json: &str,
    ) -> Result<String, HwpError> {
        let &(_, _, cell_para_idx) = path
            .last()
            .ok_or_else(|| HwpError::RenderError("경로가 비어있습니다".to_string()))?;
        // 확인 — 여기까지는 문서를 바꾸지 않는다.
        self.resolve_paragraph_by_path(section_idx, parent_para_idx, path)?;
        let size = chart_size(options_json)?;
        let (cfb, xml) = match chart_payload(chart_type, data_json) {
            Ok(v) => v,
            Err(refusal) => return refusal,
        };
        let control = self.register_chart(section_idx, size, cfb, xml);

        self.document.sections[section_idx].raw_stream = None;
        let paras = self.get_cell_paragraphs_mut_by_path(section_idx, parent_para_idx, path)?;
        let insert_idx =
            Self::insert_inline_control(&mut paras[cell_para_idx], char_offset, control);
        self.reflow_after_inline_insert_by_path(section_idx, parent_para_idx, path);
        self.event_log.push(DocumentEvent::PictureInserted {
            section: section_idx,
            para: parent_para_idx,
        });

        // 새 차트는 이 경로의 문단에서 insert_idx 번째 컨트롤이다.
        let chart_index = collect_charts(&self.document)
            .iter()
            .position(|c| {
                c.section == section_idx
                    && c.paragraph == parent_para_idx
                    && c.control == insert_idx
                    && c.container.len() == path.len()
                    && c.container
                        .iter()
                        .zip(path)
                        .all(|(level, &(ctrl, cell, para))| {
                            level.control == ctrl
                                && level.paragraph == para
                                && level.cell.unwrap_or(0) == cell
                        })
            })
            .unwrap_or_default();
        Ok(format!(
            "{{\"ok\":true,\"cellParaIdx\":{cell_para_idx},\"controlIdx\":{insert_idx},\"chartIndex\":{chart_index}}}"
        ))
    }

    /// 차트 저장소를 문서에 등록하고 넣을 차트 컨트롤을 만든다(문서 형식에 맞는 모델).
    fn register_chart(
        &mut self,
        section_idx: usize,
        (width, height): (u32, u32),
        cfb: Vec<u8>,
        xml: Vec<u8>,
    ) -> Control {
        let hwpx = matches!(self.source_format, FileFormat::Hwpx);
        let z_order = self.max_shape_z_order_in_section(section_idx) + 1;
        // 한컴 HWP 저장본의 차트 개체는 instance ID 가 0 이고, HWPX 는 `<hp:chart id>` 가 그 값이다.
        let instance_id = if hwpx {
            self.next_chart_instance_id()
        } else {
            0
        };
        let mut common = CommonObjAttr {
            width,
            height,
            z_order,
            instance_id,
            treat_as_char: false,
            flow_with_text: true,
            vert_rel_to: VertRelTo::Para,
            vert_align: VertAlign::Top,
            horz_rel_to: HorzRelTo::Column,
            horz_align: HorzAlign::Left,
            text_wrap: TextWrap::Square,
            text_flow: TextFlow::BothSides,
            numbering_type: ObjectNumberingType::Picture,
            // 한컴 HWP 저장본은 개체 속성 bit 28 을 켠다(0x140A2210).
            hwp5_gen_shape_attr_bit28: !hwpx,
            ..Default::default()
        };
        common.attr = crate::serializer::control::pack_common_attr_bits(&common);
        // 한컴은 OLE 원래·현재 크기를 7200 으로 두고 개체 크기(sz)만 차트 크기로 쓴다.
        let drawing = DrawingObjAttr {
            shape_attr: ShapeComponentAttr {
                ctrl_id: crate::parser::tags::SHAPE_OLE_ID,
                is_two_ctrl_id: true,
                local_file_version: 1,
                original_width: 7200,
                original_height: 7200,
                current_width: 7200,
                current_height: 7200,
                flip: if hwpx { 0 } else { 0x000b_0000 },
                ..Default::default()
            },
            ..Default::default()
        };
        let mut ole = OleShape {
            common,
            drawing,
            extent_x: 7200,
            extent_y: 7200,
            drawing_aspect: OleDrawingAspect::Content,
            ..Default::default()
        };

        // OLE 저장소는 HWP·HWPX 모두 DocInfo 의 저장소 레코드와 그 스트림이다(한컴 저장본을 연 모델과
        // 같다). HWPX 차트 파트(60000+N)는 순번이 아닌 id 로 찾으므로 그 앞에 끼워 순번을 맞춘다.
        let doc = &mut self.document;
        let storage_id = doc
            .bin_data_content
            .iter()
            .filter(|c| c.extension != "ooxml_chart")
            .map(|c| c.id)
            .chain(doc.doc_info.bin_data_list.iter().map(|b| b.storage_id))
            .max()
            .unwrap_or(0)
            + 1;
        let at = doc
            .bin_data_content
            .iter()
            .position(|c| c.extension == "ooxml_chart")
            .unwrap_or(doc.bin_data_content.len());
        doc.bin_data_content.insert(
            at,
            BinDataContent {
                id: storage_id,
                data: BinDataBytes::from_shared(cfb),
                extension: "OLE".to_string(),
            },
        );
        doc.doc_info.bin_data_list.push(BinData {
            raw_data: None,
            attr: if hwpx { 0 } else { 0x0002 },
            data_type: BinDataType::Storage,
            compression: BinDataCompression::Default,
            status: BinDataStatus::NotAccessed,
            abs_path: None,
            rel_path: None,
            storage_id,
            extension: Some("OLE".to_string()),
        });
        doc.doc_info.raw_stream_dirty = true;
        let position_id = at as u32 + 1;

        if hwpx {
            // `<hp:switch>`: 바깥 `<hp:chart>` 는 Chart/chartN.xml, `<hp:default><hp:ole>` 는 위 저장소다.
            let chart_num = doc
                .bin_data_content
                .iter()
                .filter(|c| c.extension == "ooxml_chart")
                .filter_map(|c| c.id.checked_sub(CHART_ID_BASE))
                .max()
                .unwrap_or(0)
                + 1;
            doc.bin_data_content.push(BinDataContent {
                id: CHART_ID_BASE + chart_num,
                data: xml.into(),
                extension: "ooxml_chart".to_string(),
            });
            let mut fallback = ole.clone();
            fallback.bin_data_id = position_id;
            fallback.common.instance_id = 0;
            fallback.hwpx_ole_id = Some(instance_id);
            // 한컴 `<hp:ole>` 의 `curSz 0`·`rotateimage="1"` 이다.
            let shape = &mut fallback.drawing.shape_attr;
            shape.current_width_was_zero = true;
            shape.current_height_was_zero = true;
            shape.rotate_image = true;
            ole.bin_data_id = u32::from(CHART_ID_BASE + chart_num);
            ole.chart_id_ref = Some(format!("Chart/chart{chart_num}.xml"));
            ole.chart_switch_fallback = Some(Box::new(fallback));
        } else {
            ole.bin_data_id = position_id;
        }
        self.bump_bin_data_epoch();
        Control::Shape(Box::new(ShapeObject::Ole(Box::new(ole))))
    }

    /// 문서 안 개체와 겹치지 않는 새 instance ID(한컴 계열 bit30).
    fn next_chart_instance_id(&self) -> u32 {
        let mut used = std::collections::HashSet::new();
        for section in &self.document.sections {
            for paragraph in &section.paragraphs {
                for control in &paragraph.controls {
                    if let Control::Shape(shape) = control {
                        used.insert(shape.common().instance_id);
                    }
                }
            }
        }
        let mut id = 0x4a39_0000u32 | (collect_charts(&self.document).len() as u32 + 1);
        while used.contains(&id) {
            id = id.wrapping_add(1) | 0x4000_0000;
        }
        id
    }
}
