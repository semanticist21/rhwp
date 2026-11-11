//! 관계형. 셀 배경의 그러데이션·그림 채우기 만들기(`setCellProperties`)와 HWP·HWPX 왕복.
//! 한컴 12.30 셀 테두리/배경 › 배경은 그러데이션(시작·끝 색, 유형 4종, 가로·세로 중심, 기울임, 번짐
//! 정도·중심)과 그림(파일, 채우기 유형, 효과, 밝기, 대비)을 고른다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::model::style::{Fill, FillType, ImageFillMode};

/// 1×1 PNG
const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0xFC, 0xCF, 0xC0, 0x50,
    0x0F, 0x00, 0x04, 0x85, 0x01, 0x80, 0x84, 0xA9, 0x8C, 0x21, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

fn cell_fill(core: &DocumentCore, cell: usize) -> Fill {
    let Control::Table(table) = &core.document().sections[0].paragraphs[0].controls[2] else {
        panic!("표");
    };
    let id = table.cells[cell].border_fill_id as usize;
    core.document().doc_info.border_fills[id - 1].fill.clone()
}

#[test]
fn cell_gradient_and_image_fills_are_created_and_round_trip() {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.create_table_native(0, 0, 0, 1, 2).unwrap();
    core.set_cell_properties_native(
        0,
        0,
        2,
        0,
        r##"{"fillType":"gradient","gradientType":2,"gradientAngle":45,"gradientCenterX":30,"gradientCenterY":70,"gradientBlur":20,"gradientStepCenter":40,"gradientStartColor":"#ff0000","gradientEndColor":"#0000ff"}"##,
    )
    .unwrap();
    let bin = core.register_image_data_native(PNG, "png");
    core.set_cell_properties_native(
        0,
        0,
        2,
        1,
        &format!(
            r#"{{"fillType":"image","imageBinDataId":{bin},"imageFillMode":"fitToSize","imageEffect":1,"imageBrightness":20,"imageContrast":-10}}"#
        ),
    )
    .unwrap();
    let check = |core: &DocumentCore, what: &str| {
        let gradient = cell_fill(core, 0);
        assert_eq!(gradient.fill_type, FillType::Gradient, "{what}");
        let g = gradient.gradient.unwrap();
        assert_eq!(
            (g.gradient_type, g.angle, g.center_x, g.center_y, g.blur),
            (2, 45, 30, 70, 20),
            "{what}"
        );
        assert_eq!(g.colors.first(), Some(&0x0000FF), "{what}: 빨강(BGR)");
        assert_eq!(g.colors.last(), Some(&0xFF0000), "{what}: 파랑(BGR)");
        let image = cell_fill(core, 1);
        assert_eq!(image.fill_type, FillType::Image, "{what}");
        let i = image.image.unwrap();
        // HWPX는 크기에 맞추어를 TOTAL로 쓴다.
        assert!(
            matches!(i.fill_mode, ImageFillMode::FitToSize | ImageFillMode::Total),
            "{what}: {:?}",
            i.fill_mode
        );
        assert_eq!(i.effect, 1, "{what}");
        assert_eq!(i.display_brightness_contrast(), (20, -10), "{what}");
        assert!(i.bin_data_id > 0, "{what}: 그림 번호");
        assert!(
            core.document()
                .bin_data_content
                .iter()
                .any(|b| b.data.load() == PNG),
            "{what}: 그림 자료"
        );
    };
    check(&core, "편집 직후");
    for (what, bytes) in [
        ("HWP", core.export_hwp_native().unwrap()),
        ("HWPX", core.export_hwpx_native().unwrap()),
    ] {
        check(&DocumentCore::from_bytes(&bytes).unwrap(), what);
    }

    // 조회한 값을 종류만 다시 쓰면(세부 값 없음) 채우기를 그대로 둔다.
    core.set_cell_properties_native(0, 0, 2, 0, r#"{"fillType":"gradient"}"#)
        .unwrap();
    check(&core, "종류만 다시 쓰기");
}
