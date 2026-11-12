//! 그림 효과 지우기(한컴 원본 그림으로)와 연결 그림 전환(삽입 그림 저장하기·문서에 포함·연결 그림 새로 고침).
//!
//! 한/글 12.30의 원본 그림으로는 그림자·네온·반사·옅은 테두리도 지운다. 효과를 지운 HWP 그림 꼬리는
//! [테두리 투명도][인스턴스 id][효과 0][원본 크기][투명도] 18바이트다. 삽입 그림 저장하기는 그림을 저장한
//! 파일에 연결하고(HWP BinData 연결, HWPX `isEmbeded="0"`), 개체 속성의 문서에 포함은 연결과 포함을
//! 오간다. 어느 쪽이든 자르기와 원본 크기(imgDim)는 그대로다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::bin_data::BinDataType;
use rhwp::model::control::Control;
use rhwp::model::image::{Picture, PictureShadow};
use rhwp::wasm_api::HwpDocument;

const PNG: &[u8] = include_bytes!("../../assets/logo/logo-32.png");
const OTHER_PNG: &[u8] = include_bytes!("../../assets/logo/logo-16.png");
const LINK: &str = "/Users/someone/그림/무제.png";

/// 한/글 12.30이 그림자·네온·옅은 테두리·반사를 건 1500×1000px 그림의 HWP 꼬리(효과 플래그 0x0b).
const HANCOM_EFFECT_TAIL: &str = "00c477380a0b000000000000000000003f0000164400003442000016440400000000000000000000000000803f0000803f000000000000000000000000000000000000003f0000fa4300000000d68261000000000006000000000048420000b4420000000000000000000000000000803f000080bf000000000000003f00000000643b7f3f3333b33e0000b44274b70100f824010000";
/// 같은 그림에 한컴이 원본 그림으로를 적용해 저장한 꼬리.
const HANCOM_CLEARED_TAIL: &str = "00c477380a0000000074b70100f824010000";

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn doc_with_picture() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_picture_native(0, 0, 0, &[], PNG, 2400, 2400, 32, 32, "png", "", None, None)
        .unwrap();
    doc
}

/// 문서의 모든 그림(본문·셀·머리말)과 그 문단·컨트롤 번호.
fn body_picture(doc: &mut HwpDocument) -> (usize, usize, &mut Picture) {
    for (pi, para) in doc.document_mut().sections[0]
        .paragraphs
        .iter_mut()
        .enumerate()
    {
        for (ci, ctrl) in para.controls.iter_mut().enumerate() {
            if let Control::Picture(pic) = ctrl {
                return (pi, ci, &mut **pic);
            }
        }
    }
    panic!("그림이 없다");
}

fn add_effects(pic: &mut Picture) {
    pic.raw_picture_extra = hex(HANCOM_EFFECT_TAIL);
    pic.effects.shadow = Some(PictureShadow::default());
    pic.effects.soft_edge_radius = Some(500.0);
}

fn assert_cleared(pic: &Picture) {
    assert_eq!(pic.raw_picture_extra, hex(HANCOM_CLEARED_TAIL));
    assert_eq!(pic.img_dim, (112_500, 75_000));
    assert!(pic.effects.shadow.is_none() && pic.effects.soft_edge_radius.is_none());
}

#[test]
fn clear_picture_effects_matches_hancom_tail_in_body_and_header() {
    let mut doc = doc_with_picture();
    let (para, ctrl, pic) = body_picture(&mut doc);
    add_effects(pic);
    let result = doc
        .clear_picture_effects_native(0, para, &[], ctrl)
        .unwrap();
    assert!(result.contains("\"changed\":true"), "{result}");
    assert_cleared(body_picture(&mut doc).2);
    let again = doc
        .clear_picture_effects_native(0, para, &[], ctrl)
        .unwrap();
    assert!(again.contains("\"changed\":false"), "{again}");
    let reopened = &mut HwpDocument::from_bytes(&doc.export_hwp().unwrap()).unwrap();
    assert_cleared(body_picture(reopened).2);
    let hwpx = &mut HwpDocument::from_bytes(&doc.export_hwpx().unwrap()).unwrap();
    let pic = body_picture(hwpx).2;
    assert!(pic.effects.shadow.is_none() && pic.effects.soft_edge_radius.is_none());

    // 머리말 그림은 경로의 머리말 단계(셀 0, 머리말 문단)로 가리킨다.
    let mut doc = doc_with_picture();
    let header: serde_json::Value =
        serde_json::from_str(&doc.create_header_footer_native(0, true, 0).unwrap()).unwrap();
    let (hp, hc) = (
        header["paraIndex"].as_u64().unwrap() as usize,
        header["controlIndex"].as_u64().unwrap() as usize,
    );
    let mut copy = body_picture(&mut doc).2.clone();
    add_effects(&mut copy);
    let host = &mut doc.document_mut().sections[0].paragraphs[hp].controls[hc];
    let Control::Header(h) = host else {
        panic!("머리말이 아니다");
    };
    h.paragraphs[0]
        .controls
        .push(Control::Picture(Box::new(copy)));
    let inner = h.paragraphs[0].controls.len() - 1;
    doc.clear_picture_effects_native(0, hp, &[(hc, 0, 0)], inner)
        .unwrap();
    let Control::Header(h) = &doc.document().sections[0].paragraphs[hp].controls[hc] else {
        unreachable!()
    };
    let Control::Picture(pic) = &h.paragraphs[0].controls[inner] else {
        panic!("머리말 그림이 아니다");
    };
    assert_cleared(pic);
}

fn props(doc: &HwpDocument, para: usize, ctrl: usize) -> serde_json::Value {
    serde_json::from_str(&doc.get_picture_properties_native(0, para, ctrl).unwrap()).unwrap()
}

#[test]
fn link_then_embed_keeps_crop_and_round_trips() {
    let mut doc = doc_with_picture();
    let (para, ctrl, pic) = body_picture(&mut doc);
    pic.crop.left = 600;
    let crop = pic.crop;
    doc.link_picture_image_native(0, para, &[], ctrl, LINK)
        .unwrap();
    assert_eq!(props(&doc, para, ctrl)["externalPath"], LINK);
    assert_eq!(
        doc.get_control_image_data_native(0, para, &[], ctrl)
            .unwrap(),
        PNG,
        "연결한 뒤에도 보이는 그림은 같다"
    );
    assert!(doc
        .link_picture_image_native(0, para, &[], ctrl, LINK)
        .is_err());

    for bytes in [doc.export_hwp().unwrap(), doc.export_hwpx().unwrap()] {
        let mut reopened = HwpDocument::from_bytes(&bytes).unwrap();
        let list = &reopened.document().doc_info.bin_data_list;
        assert_eq!(
            list.len(),
            1,
            "이 그림만 쓰던 이미지는 제자리에서 연결로 바뀐다"
        );
        assert_eq!(list[0].data_type, BinDataType::Link);
        assert_eq!(list[0].abs_path.as_deref(), Some(LINK));
        let (p, c, pic) = body_picture(&mut reopened);
        assert_eq!((pic.crop.left, pic.crop.right), (crop.left, crop.right));
        assert_eq!(pic.img_dim, (32 * 75, 32 * 75));

        // 연결 그림 새로 고침: 처음 읽기와 다시 읽기 모두 받는다.
        let refs: Vec<serde_json::Value> =
            serde_json::from_str(&reopened.get_external_image_references()).unwrap();
        let key = refs[0]["key"].as_str().unwrap().to_string();
        assert_eq!(reopened.reload_external_image_by_key(&key, PNG, LINK), 1);
        assert_eq!(
            reopened.reload_external_image_by_key(&key, OTHER_PNG, LINK),
            1
        );
        assert_eq!(
            reopened
                .get_control_image_data_native(0, p, &[], c)
                .unwrap(),
            OTHER_PNG
        );
        assert_eq!(
            reopened.reload_external_image_by_key("binData:9", PNG, ""),
            0
        );

        // 문서에 포함 켜기: 읽은 바이트를 넣고 자르기는 그대로다.
        reopened.embed_picture_image_native(0, p, &[], c).unwrap();
        let embedded = &mut HwpDocument::from_bytes(&reopened.export_hwp().unwrap()).unwrap();
        let list = &embedded.document().doc_info.bin_data_list;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].data_type, BinDataType::Embedding);
        let (p, c, pic) = body_picture(embedded);
        assert!(pic.image_attr.external_path.is_none());
        assert_eq!(pic.crop.left, crop.left);
        assert_eq!(
            embedded
                .get_control_image_data_native(0, p, &[], c)
                .unwrap(),
            OTHER_PNG
        );
    }
}

#[test]
fn linking_a_shared_image_keeps_the_other_picture_embedded() {
    let mut doc = doc_with_picture();
    doc.insert_picture_native(
        0,
        0,
        0,
        &[],
        OTHER_PNG,
        1200,
        1200,
        16,
        16,
        "png",
        "",
        None,
        None,
    )
    .unwrap();
    // 두 그림이 첫 이미지를 함께 쓰게 한다.
    let pictures = all_pictures(&doc);
    assert_eq!(pictures.len(), 2);
    let [(p0, c0), (p1, c1)] = [pictures[0], pictures[1]];
    let shared = match &doc.document().sections[0].paragraphs[p0].controls[c0] {
        Control::Picture(pic) => pic.image_attr.bin_data_id,
        _ => unreachable!(),
    };
    if let Control::Picture(pic) = &mut doc.document_mut().sections[0].paragraphs[p1].controls[c1] {
        pic.image_attr.bin_data_id = shared;
    }
    let shared_bytes = doc.get_control_image_data_native(0, p0, &[], c0).unwrap();
    doc.link_picture_image_native(0, p0, &[], c0, LINK).unwrap();
    let mut reopened = HwpDocument::from_bytes(&doc.export_hwp().unwrap()).unwrap();
    assert_eq!(all_pictures(&reopened), pictures);
    let kinds: Vec<BinDataType> = reopened
        .document()
        .doc_info
        .bin_data_list
        .iter()
        .map(|bd| bd.data_type)
        .collect();
    assert_eq!(kinds.first(), Some(&BinDataType::Embedding));
    assert_eq!(kinds.last(), Some(&BinDataType::Link), "{kinds:?}");
    assert_eq!(props(&reopened, p0, c0)["externalPath"], LINK);
    assert!(props(&reopened, p1, c1)["externalPath"].is_null());
    assert_eq!(
        reopened
            .get_control_image_data_native(0, p1, &[], c1)
            .unwrap(),
        shared_bytes,
        "함께 쓰던 다른 그림은 넣은 채로 남는다"
    );
    // 연결 파일을 읽기 전이라 문서에 넣기는 거절한다.
    assert!(reopened.embed_picture_image_native(0, p0, &[], c0).is_err());
}

fn all_pictures(doc: &HwpDocument) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    for (pi, para) in doc.document().sections[0].paragraphs.iter().enumerate() {
        for (ci, ctrl) in para.controls.iter().enumerate() {
            if matches!(ctrl, Control::Picture(_)) {
                found.push((pi, ci));
            }
        }
    }
    found
}
