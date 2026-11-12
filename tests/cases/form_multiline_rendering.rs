//! 저장된 개행과 입력 상자의 표시 문자열을 구분한다.
//! 한/글 12.30 Mac은 FIRST LF SECOND(CR LF·CR도 같음) 입력을 MultiLine=0/1 모두
//! `FIRSTSECOND` 한 줄로 그린다(화면과 PDF로 저장하기). 한/글 2022 Windows Print는 FIRST만
//! 보였으나(#7589) 앱의 기준은 12.30 Mac이다.
use rhwp::document_core::DocumentCore;
use rhwp::model::control::{Control, FormType};

fn fixture(multiline: bool, text: &str) -> DocumentCore {
    let mut core =
        DocumentCore::from_bytes(include_bytes!("../../samples/hwpx/form-01.hwpx")).unwrap();
    let Control::Form(form) = &mut core.document_mut().sections[0].paragraphs[8].controls[0] else {
        panic!("공개 form-01의 Edit 컨트롤")
    };
    assert_eq!(form.form_type, FormType::Edit);
    form.text = text.into();
    form.properties
        .insert("MultiLine".into(), if multiline { "1" } else { "0" }.into());
    form.properties.insert("PasswordChar".into(), "".into());
    // JSON setter의 별도 escaping 결함을 거치지 않고 실제 저장된 개행을 파싱한다.
    let reopened = DocumentCore::from_bytes(&core.export_hwpx_native().unwrap()).unwrap();
    let Control::Form(form) = &reopened.document().sections[0].paragraphs[8].controls[0] else {
        unreachable!()
    };
    assert_eq!(form.text, text);
    assert_eq!(
        form.properties["MultiLine"],
        if multiline { "1" } else { "0" }
    );
    reopened
}

#[test]
fn edit_displays_stored_lines_joined_for_both_multiline_values() {
    let multiline = fixture(true, "FIRST\nSECOND");
    let singleline = fixture(false, "FIRST\nSECOND");
    let outputs = [multiline, singleline].map(|doc| doc.render_page_svg_native(0).unwrap());
    // 원문·속성 보존은 fixture에서, 실제 표시 문자열은 출력에서 독립적으로 확인한다.
    for svg in &outputs {
        assert!(
            svg.contains(">FIRSTSECOND</text>"),
            "줄 나눔 없이 줄들을 이어 그린다"
        );
        assert!(!svg.contains("FIRST\n"), "저장된 개행 문자는 그리지 않는다");
    }
    assert_eq!(outputs[0], outputs[1], "MultiLine 값과 상관없이 같다");
}

#[test]
fn edit_password_mask_counts_every_stored_line() {
    let mut doc = fixture(true, "AB\nCD");
    let Control::Form(form) = &mut doc.document_mut().sections[0].paragraphs[8].controls[0] else {
        unreachable!()
    };
    form.properties.insert("PasswordChar".into(), "*".into());
    let svg = doc.render_page_svg_native(0).unwrap();
    assert!(svg.contains(">****</text>"), "줄마다의 글자를 모두 가린다");
}

#[test]
fn multiline_edit_without_line_break_keeps_single_line_output() {
    assert_eq!(
        fixture(true, "SINGLE").render_page_svg_native(0).unwrap(),
        fixture(false, "SINGLE").render_page_svg_native(0).unwrap(),
    );
}
