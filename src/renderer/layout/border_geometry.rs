//! 저장 간격이 정하는 쪽 테두리 중심과 물리 선 묶음의 기하를 분리한다.
use crate::model::page::{PageBorderBasis, PageBorderFill};
use crate::renderer::{
    hwpunit_to_px,
    page_layout::{LayoutRect, PageLayoutInfo},
    DEFAULT_DPI,
};

/// 선언 굵기는 이중선 전체 폭이다. 각 선은 전체 폭의 1/4, 빈 간격은 각 선의 두 배다.
/// 독립 한컴 Print의 1/600 inch 격자로 각 stroke를 반올림한다.
/// 반환 좌표는 기존 선 생성자의 96dpi 공간이며 화면 최소 px 폭을 추가하지 않는다.
pub(crate) fn double_line_parts(declared_width: f64) -> [(f64, f64); 2] {
    let grid = DEFAULT_DPI / 600.0;
    let stroke = (declared_width / (4.0 * grid)).round().max(1.0) * grid;
    let offset = stroke * 1.5;
    [(-offset, stroke), (offset, stroke)]
}

/// 선 묶음 중심의 박스. 선의 굵기와 인쇄 잉크 범위는 저장 간격을 바꾸지 않는다.
/// paint와 Studio 쪽 정보는 같은 실제 페이지·본문 영역을 소비한다.
pub(crate) fn page_border_rect(
    pbf: &PageBorderFill,
    layout: &PageLayoutInfo,
    dpi: f64,
) -> LayoutRect {
    let left = hwpunit_to_px(i32::from(pbf.spacing_left), dpi);
    let right = hwpunit_to_px(i32::from(pbf.spacing_right), dpi);
    let top = hwpunit_to_px(i32::from(pbf.spacing_top), dpi);
    let bottom = hwpunit_to_px(i32::from(pbf.spacing_bottom), dpi);
    match pbf.basis {
        PageBorderBasis::PaperBased => LayoutRect {
            x: left,
            y: top,
            width: layout.page_width - left - right,
            height: layout.page_height - top - bottom,
        },
        PageBorderBasis::BodyBased => {
            // 머리말·꼬리말 포함(attr bit 1·2)은 테두리를 머리말 위·꼬리말 아래까지 넓힌다
            // (한/글 12.30 PDF로 저장하기: 위·아래로 머리말·꼬리말 높이만큼, 2026-11-12).
            let body_top = layout.body_area.y;
            let body_bottom = layout.body_area.y + layout.body_area.height;
            let area_top = if pbf.attr & 0x02 != 0 {
                layout.header_area.y.min(body_top)
            } else {
                body_top
            };
            let area_bottom = if pbf.attr & 0x04 != 0 {
                (layout.footer_area.y + layout.footer_area.height).max(body_bottom)
            } else {
                body_bottom
            };
            LayoutRect {
                x: layout.body_area.x - left,
                y: area_top - top,
                width: layout.body_area.width + left + right,
                height: area_bottom - area_top + top + bottom,
            }
        }
    }
}
