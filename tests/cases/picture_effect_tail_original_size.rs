//! 한컴 그림 효과 꼬리(그림자·네온·반사)의 원본 크기를 효과 정보 뒤에서 읽는다.
//!
//! HWP5 그림 꼬리는 [테두리 투명도][인스턴스 id][효과 플래그][효과별 정보][원본 크기][투명도]다.
//! 효과별 정보는 플래그마다 길이가 달라, 고정 위치 9..17을 원본 크기로 읽으면 그림자 값이
//! 원본 크기가 돼 자르기 기준이 어긋난다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::control::Control;
use rhwp::wasm_api::HwpDocument;

const PNG: &[u8] = include_bytes!("../../assets/logo/logo-32.png");

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn picture_tail(doc: &mut HwpDocument) -> &mut rhwp::model::image::Picture {
    for para in doc.document_mut().sections[0].paragraphs.iter_mut() {
        for ctrl in para.controls.iter_mut() {
            if let Control::Picture(pic) = ctrl {
                return pic;
            }
        }
    }
    panic!("그림이 없다");
}

#[test]
fn hancom_effect_tail_keeps_original_size_after_effects() {
    // 한컴 12.30이 그림자·네온·반사를 건 1500×1000px 그림 꼬리(효과 플래그 0x0b).
    let tail = hex("00c477380a0b000000000000000000003f0000164400003442000016440400000000000000000000000000803f0000803f000000000000000000000000000000000000003f0000fa4300000000d68261000000000006000000000048420000b4420000000000000000000000000000803f000080bf000000000000003f00000000643b7f3f3333b33e0000b44274b70100f824010000");
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_picture_native(0, 0, 0, &[], PNG, 2400, 2400, 32, 32, "png", "", None, None)
        .unwrap();
    picture_tail(&mut doc).raw_picture_extra = tail.clone();
    let mut reopened = HwpDocument::from_bytes(&doc.export_hwp().unwrap()).unwrap();
    let pic = picture_tail(&mut reopened);
    assert_eq!(pic.raw_picture_extra, tail);
    assert_eq!(pic.img_dim, (112_500, 75_000));
    assert_eq!(pic.image_attr.transparency, 0);
}
