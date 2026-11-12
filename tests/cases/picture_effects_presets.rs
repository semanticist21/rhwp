//! 그림 도구 상자의 빠른 효과(그림자·반사·네온·옅은 테두리·그림 스타일) 견본을 한컴처럼 적는다.
//!
//! 한/글 12.30 Mac에서 같은 그림에 견본을 걸어 저장한 HWP 꼬리의 효과 정보와 HWPX `hp:effects`를
//! 기준으로 삼는다. HWP 꼬리는 [플래그][그림자 56][네온 20+8n][옅은 테두리 4][반사 56]이고, 그 뒤
//! 원본 크기·투명도는 그대로 남는다. 네온은 HWPX가 전에 버리던 효과다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::control::Control;
use rhwp::model::image::Picture;
use rhwp::wasm_api::HwpDocument;
use std::io::{Cursor, Read};

const PNG: &[u8] = include_bytes!("../../assets/logo/logo-32.png");

/// 한컴 바깥쪽 그림자 1번(오른쪽 아래 45도) 효과 정보.
const SHADOW_45: &str = "01000000000000000000003f0000164400003442000016440400000000000000000000000000803f0000803f00000000000000000000000000000000";
/// 한컴 네온 3줄 6번(15pt 보라, 채도 1.75배).
const GLOW_PURPLE: &str = "020000000000003f0080bb4400000000bb5c9d000100000004020000";
const GLOW_PURPLE_TAIL: &str = "0000e03f";
/// 한컴 반사 1번(근접 1/3쯤, 간격 0).
const REFLECTION_TIGHT: &str = "0800000006000000000048420000b4420000000000000000000000000000803f000080bf000000000000003f00000000643b7f3f3333b33e0000b442";
/// 한컴 옅은 테두리 6번(30pt).
const SOFT_30: &str = "0400000000803b45";

fn hex(text: &str) -> Vec<u8> {
    let text: String = text.split_whitespace().collect();
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

fn picture(doc: &mut HwpDocument) -> (usize, usize, &mut Picture) {
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

/// HWP로 저장해 다시 연 그림 꼬리의 효과 정보(인스턴스 id 뒤부터 원본 크기·투명도 앞까지).
fn saved_effect_bytes(doc: &HwpDocument) -> Vec<u8> {
    let mut reopened = HwpDocument::from_bytes(&doc.export_hwp().unwrap()).unwrap();
    let tail = picture(&mut reopened).2.raw_picture_extra.clone();
    tail[5..tail.len() - 9].to_vec()
}

fn section_xml(bytes: &[u8]) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    archive
        .by_name("Contents/section0.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    xml
}

const SHADOW_JSON: &str = r##"{"shadow":{"style":"OUTSIDE","alpha":0.5,"radius":600,"direction":45,"distance":600,"alignStyle":"CENTER","skewX":0,"skewY":0,"scaleX":1,"scaleY":1,"color":"#000000"}}"##;
const GLOW_JSON: &str = r##"{"glow":{"alpha":0.5,"radius":1500,"color":"#9d5cbb","colorEffects":[{"type":"SAT_MOD","value":1.75}]}}"##;
const REFLECTION_JSON: &str = r##"{"reflection":{"alignStyle":"BOTTOM_LEFT","radius":50,"direction":90,"distance":0,"fadeDirection":90,"skewX":0,"skewY":0,"scaleX":1,"scaleY":-1,"startAlpha":0.5,"endAlpha":0.997,"startPos":0,"endPos":0.35}}"##;

#[test]
fn each_quick_effect_saves_hancom_hwp_bytes() {
    let glow = [hex(GLOW_PURPLE), hex(GLOW_PURPLE_TAIL)].concat();
    for (json, expected) in [
        (SHADOW_JSON, hex(SHADOW_45)),
        (GLOW_JSON, glow),
        (REFLECTION_JSON, hex(REFLECTION_TIGHT)),
        (r#"{"softEdge":3000}"#, hex(SOFT_30)),
    ] {
        let mut doc = doc_with_picture();
        let (para, ctrl, _) = picture(&mut doc);
        doc.set_picture_effects_native(0, para, &[], ctrl, json)
            .unwrap();
        assert_eq!(saved_effect_bytes(&doc), expected, "{json}");
    }
}

#[test]
fn effects_combine_in_hancom_order_and_null_removes_one() {
    let mut doc = doc_with_picture();
    let (para, ctrl, _) = picture(&mut doc);
    for json in [
        SHADOW_JSON,
        GLOW_JSON,
        r#"{"softEdge":3000}"#,
        REFLECTION_JSON,
    ] {
        doc.set_picture_effects_native(0, para, &[], ctrl, json)
            .unwrap();
    }
    let mut all = hex(SHADOW_45);
    all[0] = 0x0f;
    all.extend(&hex(GLOW_PURPLE)[4..]);
    all.extend(hex(GLOW_PURPLE_TAIL));
    all.extend(&hex(SOFT_30)[4..]);
    all.extend(&hex(REFLECTION_TIGHT)[4..]);
    assert_eq!(saved_effect_bytes(&doc), all);

    // 저장해 다시 연 HWP에서 그림자만 지우면 원문 꼬리의 나머지 효과와 원본 크기가 남는다.
    let mut reopened = HwpDocument::from_bytes(&doc.export_hwp().unwrap()).unwrap();
    let dim = picture(&mut reopened).2.img_dim;
    reopened
        .set_picture_effects_native(0, para, &[], ctrl, r#"{"shadow":null}"#)
        .unwrap();
    let mut rest = all[..4].to_vec();
    rest[0] = 0x0e;
    rest.extend(&all[60..]);
    assert_eq!(saved_effect_bytes(&reopened), rest);
    assert_eq!(picture(&mut reopened).2.img_dim, dim);
    let effects: serde_json::Value = serde_json::from_str(
        &reopened
            .get_picture_effects_native(0, para, &[], ctrl)
            .unwrap(),
    )
    .unwrap();
    assert!(effects["shadow"].is_null());
    assert_eq!(effects["glow"]["color"], "#9d5cbb");
    assert_eq!(effects["glow"]["colorEffects"][0]["value"], 1.75);
    assert_eq!(effects["softEdge"], 3000.0);
    assert_eq!(effects["reflection"]["endPos"], 0.35);
}

#[test]
fn hwpx_keeps_glow_and_reflection_like_hancom() {
    let mut doc = doc_with_picture();
    let (para, ctrl, _) = picture(&mut doc);
    for json in [GLOW_JSON, REFLECTION_JSON] {
        doc.set_picture_effects_native(0, para, &[], ctrl, json)
            .unwrap();
    }
    let bytes = doc.export_hwpx().unwrap();
    let xml = section_xml(&bytes);
    // 한컴 12.30 저장본과 같은 꼴이다.
    assert!(xml.contains(r#"<hp:glow alpha="0.5" radius="1500"><hp:effectsColor type="RGB" schemeIdx="-1" systemIdx="-1" presetIdx="-1"><hp:rgb r="157" g="92" b="187"/><hp:effect type="SAT_MOD" value="1.75"/></hp:effectsColor></hp:glow>"#), "{xml}");
    assert!(xml.contains(r#"<hp:reflection alignStyle="BOTTOM_LEFT" radius="50" direction="90" distance="0" rotationStyle="0" fadeDirection="90"><hp:skew x="0" y="0"/><hp:scale x="1" y="-1"/><hp:alpha start="0.5" end="0.997"/><hp:pos start="0" end="0.35"/></hp:reflection>"#), "{xml}");

    // HWPX를 다시 열면 효과가 남고, 그 문서를 HWP로 저장하면 한컴 꼬리가 된다.
    let mut reopened = HwpDocument::from_bytes(&bytes).unwrap();
    let (para, ctrl, _) = picture(&mut reopened);
    let effects: serde_json::Value = serde_json::from_str(
        &reopened
            .get_picture_effects_native(0, para, &[], ctrl)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(effects["glow"]["radius"], 1500.0);
    assert_eq!(effects["reflection"]["scaleY"], -1.0);
    let mut expected = hex(GLOW_PURPLE);
    expected[0] = 0x0a;
    expected.extend(hex(GLOW_PURPLE_TAIL));
    expected.extend(&hex(REFLECTION_TIGHT)[4..]);
    assert_eq!(saved_effect_bytes(&reopened), expected);
}

#[test]
fn picture_style_border_writes_hancom_line() {
    // 한컴 그림 스타일 1번: 검은 실선 3pt(850)·끝 모양 평평·바깥 외곽선. HWP 테두리 속성 0x41.
    let mut doc = doc_with_picture();
    let (para, ctrl, _) = picture(&mut doc);
    doc.set_picture_properties_native(
        0,
        para,
        ctrl,
        r#"{"borderColor":0,"borderWidth":850,"lineType":1,"lineEndShape":1,"borderOutlineStyle":1}"#,
    )
    .unwrap();
    let mut reopened = HwpDocument::from_bytes(&doc.export_hwp().unwrap()).unwrap();
    let pic = picture(&mut reopened).2;
    assert_eq!((pic.border_attr.attr, pic.border_width), (0x41, 850));
    let xml = section_xml(&doc.export_hwpx().unwrap());
    assert!(xml.contains(r##"<hp:lineShape color="#000000" width="850" style="SOLID" endCap="FLAT" headStyle="NORMAL" tailStyle="NORMAL" headfill="0" tailfill="0" headSz="SMALL_SMALL" tailSz="SMALL_SMALL" outlineStyle="OUTER" alpha="0"/>"##), "{xml}");
    let props: serde_json::Value =
        serde_json::from_str(&doc.get_picture_properties_native(0, para, ctrl).unwrap()).unwrap();
    assert_eq!(
        (
            props["borderAttr"].clone(),
            props["borderOutlineStyle"].clone()
        ),
        (serde_json::json!(0x41), serde_json::json!(1))
    );
}
