//! 도형 선 투명도(HWP 인스턴스 ID 다음 바이트, HWPX `lineShape@alpha`)와 HWPX 채우기 투명도를
//! 한컴 12.30처럼 0~255 정수로 읽고 쓰는지 검사한다. 한컴 12.30에서 선 투명도 50%·채우기 30%를
//! 준 사각형은 HWPX에 `lineShape alpha="128"`·`winBrush alpha="77"`로, HWP에 같은 바이트로 저장된다.

use crate::document_core::DocumentCore;

fn shape_props(core: &DocumentCore) -> serde_json::Value {
    serde_json::from_str(
        &core
            .get_shape_properties_native(0, 0, 0)
            .expect("도형 속성"),
    )
    .expect("JSON")
}

fn rectangle_with_alpha() -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().expect("빈 문서");
    core.create_shape_control_native(
        0,
        0,
        0,
        9000,
        6000,
        20000,
        20000,
        false,
        "InFrontOfText",
        "rectangle",
        false,
        false,
        &[],
    )
    .expect("사각형");
    let control = shape_control(&core);
    core.set_shape_properties_native(
        0,
        0,
        control,
        r#"{"lineAlpha":128,"fillType":"solid","fillBgColor":255,"fillAlpha":77}"#,
    )
    .expect("투명도");
    core
}

#[test]
fn line_and_fill_alpha_survive_hwp_and_hwpx() {
    let core = rectangle_with_alpha();
    assert_eq!(shape_props(&core)["lineAlpha"], 128);
    for bytes in [
        core.export_hwp_native().expect("HWP"),
        core.export_hwpx_native().expect("HWPX"),
    ] {
        let reopened = DocumentCore::from_bytes(&bytes).expect("다시 열기");
        let props = shape_props(&reopened);
        assert_eq!(props["lineAlpha"], 128);
        assert_eq!(props["fillAlpha"], 77);
    }
}

#[test]
fn hwpx_writes_alpha_as_hancom_integers() {
    let bytes = rectangle_with_alpha().export_hwpx_native().expect("HWPX");
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("zip");
    let mut xml = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("Contents/section0.xml").expect("section0"),
        &mut xml,
    )
    .expect("읽기");
    assert!(
        xml.contains(r#"outlineStyle="NORMAL" alpha="128""#),
        "{xml}"
    );
    assert!(xml.contains(r#"alpha="77"/></hc:fillBrush>"#), "{xml}");
}

#[test]
fn hancom_integer_alpha_is_a_byte() {
    use crate::parser::hwpx::utils::parse_alpha_byte;
    assert_eq!(parse_alpha_byte("77"), Some(77));
    assert_eq!(parse_alpha_byte("128"), Some(128));
    assert_eq!(parse_alpha_byte("0"), Some(0));
    // 예전 rhwp 저장본의 비율 값
    assert_eq!(parse_alpha_byte("0.30196078431372547"), Some(77));
    assert_eq!(parse_alpha_byte("x"), None);
}

#[test]
fn line_alpha_draws_only_the_stroke_transparent() {
    let core = rectangle_with_alpha();
    let svg = core.render_page_svg_native(0).expect("SVG");
    let rect = svg
        .lines()
        .find(|line| line.starts_with("<rect") && line.contains("#ff0000"))
        .expect("사각형");
    assert!(rect.contains(r#"stroke-opacity="0.498""#), "{rect}");
    assert!(rect.contains(r#"fill-opacity="0.698""#), "{rect}");
    assert!(!rect.contains(" opacity="), "{rect}");
}
