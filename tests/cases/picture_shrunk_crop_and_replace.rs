//! 관계형. 넣을 때 줄인 그림(imgDim = 픽셀×75가 원래 크기보다 큼)의 자르기와 그림 이미지 바꾸기.
//! 한컴 12.30은 줄인 그림도 자르기 양을 imgDim 단위로 보이고 적는다(3000px 그림을 150mm로 넣은 문서:
//! 확대/축소 비율 18.9%, 오른쪽 10mm 자르기 → imgClip 끝 222166 = 225000 − 10mm, 틀 너비 42520 → 41984). 그림 줄이기는 이미지를
//! 바꾸고 옛 BinData를 버린다(892 KB → 306 KB).
//! 기준: 자르기 양 = imgDim − imgClip 끝이고, 바꾼 이미지 바이트만 저장본에 남는다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::model::image::Picture;

const LOGO: &[u8] = include_bytes!("../../assets/logo/logo-16.png");

/// 16px 그림을 틀 `size`(HWPUNIT)로 넣은 문서와 그 그림의 (문단, 컨트롤).
fn with_picture(size: u32) -> (DocumentCore, usize, usize) {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.insert_picture_native(
        0,
        0,
        0,
        &[],
        LOGO,
        size,
        size,
        16,
        16,
        "png",
        "",
        None,
        None,
    )
    .unwrap();
    let (para, ctrl) = locate(&core);
    (core, para, ctrl)
}

fn locate(core: &DocumentCore) -> (usize, usize) {
    core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .find_map(|(pi, p)| {
            p.controls
                .iter()
                .position(|c| matches!(c, Control::Picture(_)))
                .map(|ci| (pi, ci))
        })
        .unwrap()
}

fn picture(core: &DocumentCore) -> &Picture {
    let (pi, ci) = locate(core);
    let Control::Picture(p) = &core.document().sections[0].paragraphs[pi].controls[ci] else {
        unreachable!()
    };
    p
}

fn crop_right_ui(core: &DocumentCore, para: usize, ctrl: usize) -> i64 {
    let v: serde_json::Value =
        serde_json::from_str(&core.get_picture_properties_native(0, para, ctrl).unwrap()).unwrap();
    v["cropRight"].as_i64().unwrap()
}

fn reopened(core: &DocumentCore) -> [(&'static str, DocumentCore); 2] {
    [
        (
            "HWP",
            DocumentCore::from_bytes(&core.export_hwp_native().unwrap()).unwrap(),
        ),
        (
            "HWPX",
            DocumentCore::from_bytes(&core.export_hwpx_native().unwrap()).unwrap(),
        ),
    ]
}

#[test]
fn crop_amounts_use_img_dim_units_for_shrunk_pictures() {
    // 16px × 75 = 1200, 틀 600: 반으로 줄여 넣은 그림. 원래 크기(orgSz)는 틀과 같다(600).
    let (mut core, para, ctrl) = with_picture(600);
    assert_eq!(picture(&core).img_dim, (1200, 1200));
    assert_eq!(crop_right_ui(&core, para, ctrl), 0);
    let v: serde_json::Value =
        serde_json::from_str(&core.get_picture_properties_native(0, para, ctrl).unwrap()).unwrap();
    assert_eq!(
        (v["cropExtentWidth"].as_u64(), v["originalWidth"].as_u64()),
        (Some(1200), Some(600))
    );
    core.set_picture_properties_native(0, para, ctrl, r#"{"cropRight":150}"#)
        .unwrap();
    assert_eq!(
        picture(&core).crop.right,
        1200 - 150,
        "자르기 양은 imgDim 단위"
    );
    assert_eq!(crop_right_ui(&core, para, ctrl), 150);
    for (name, doc) in reopened(&core) {
        let (pi, ci) = locate(&doc);
        assert_eq!(picture(&doc).crop.right, 1050, "{name}");
        assert_eq!(crop_right_ui(&doc, pi, ci), 150, "{name}");
    }
    // 원래 끝을 넘어 되돌리면 원래 끝에서 멈춘다.
    core.set_picture_properties_native(0, para, ctrl, r#"{"cropRight":-50}"#)
        .unwrap();
    assert_eq!(picture(&core).crop.right, 1200);
}

#[test]
fn replacing_sole_picture_image_drops_old_bytes() {
    let (mut core, para, ctrl) = with_picture(600);
    let before = core.document().bin_data_content.len();
    let tiny: Vec<u8> = {
        // 같은 PNG의 바이트를 바꾼 다른 이미지로 둔다(디코딩은 검사하지 않는다).
        let mut v = LOGO.to_vec();
        v.extend_from_slice(b"shrunk");
        v
    };
    let r: serde_json::Value = serde_json::from_str(
        &core
            .replace_picture_image_native(0, para, &[], ctrl, &tiny, 8, 8, "png")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(r["replacedInPlace"], true);
    assert_eq!(core.document().bin_data_content.len(), before);
    let pic = picture(&core);
    assert_eq!(pic.img_dim, (600, 600));
    assert_eq!((pic.crop.right, pic.crop.bottom), (600, 600));
    assert_eq!(pic.shape_attr.original_width, 600, "원래 크기는 그대로");
    for (name, doc) in reopened(&core) {
        assert_eq!(doc.document().bin_data_content.len(), 1, "{name}");
        let pic = picture(&doc);
        let bytes = doc.document().bin_data_content[usize::from(pic.image_attr.bin_data_id) - 1]
            .data
            .load();
        assert_eq!(bytes, tiny, "{name}: 바꾼 이미지");
    }

    // HWP에서 연 그림(원본 꼬리 바이트가 있는 그림)도 새 imgDim으로 저장한다.
    let mut hwp = reopened(&core).into_iter().next().unwrap().1;
    let (pi, ci) = locate(&hwp);
    hwp.replace_picture_image_native(0, pi, &[], ci, LOGO, 16, 16, "png")
        .unwrap();
    for (name, doc) in reopened(&hwp) {
        let pic = picture(&doc);
        assert_eq!(pic.img_dim, (1200, 1200), "{name}");
        assert_eq!((pic.crop.right, pic.crop.bottom), (1200, 1200), "{name}");
    }

    // 같은 BinData를 두 그림이 쓰면 새 BinData를 달고 다른 그림은 그대로 둔다.
    let (mut core, _, _) = with_picture(600);
    let shared = picture(&core).image_attr.bin_data_id;
    core.insert_picture_native(0, 0, 0, &[], LOGO, 600, 600, 16, 16, "png", "", None, None)
        .unwrap();
    for p in &mut core.document_mut().sections[0].paragraphs {
        for c in &mut p.controls {
            if let Control::Picture(pic) = c {
                pic.image_attr.bin_data_id = shared;
            }
        }
    }
    let (para2, ctrl2) = locate(&core);
    let r: serde_json::Value = serde_json::from_str(
        &core
            .replace_picture_image_native(0, para2, &[], ctrl2, &tiny, 8, 8, "png")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(r["replacedInPlace"], false);
    assert_ne!(r["binDataId"].as_u64().unwrap(), u64::from(shared));
}

/// HWPX에서 온 그림을 HWP로 저장하면 한컴처럼 imgDim 자리에 0을 쓴다. 다시 열면 자르기 범위를
/// 원래 크기(orgSz)로 재지 않고 렌더러처럼 픽셀 크기에서 재야 한다. 한컴 12.30은 이 HWP의
/// 그림을 자르지 않은 전체로, 왼쪽을 자른 그림은 오른쪽 반으로 그린다.
#[test]
fn crop_extent_without_img_dim_follows_decoded_pixels() {
    // 16px × 75 = 1200 범위를 틀 20000으로 키워 넣는다. 저장은 앱(wasm exportHwp)과 같은 어댑터 경로다.
    let (core, para, ctrl) = with_picture(20000);
    let from_hwpx = |core: &DocumentCore| {
        let hwpx = DocumentCore::from_bytes(&core.export_hwpx_native().unwrap()).unwrap();
        DocumentCore::from_bytes(&hwpx.export_hwp_with_adapter_snapshot().unwrap()).unwrap()
    };
    let props = |core: &DocumentCore| -> serde_json::Value {
        serde_json::from_str(&core.get_picture_properties_native(0, para, ctrl).unwrap()).unwrap()
    };
    let hwp = from_hwpx(&core);
    assert_eq!(picture(&hwp).img_dim, (0, 0), "HWPX→HWP는 imgDim 0을 쓴다");
    let v = props(&hwp);
    assert_eq!(
        (&v["cropExtentWidth"], &v["cropRight"], &v["cropBottom"]),
        (
            &serde_json::json!(1200),
            &serde_json::json!(0),
            &serde_json::json!(0)
        )
    );

    let mut hwpx = DocumentCore::from_bytes(&core.export_hwpx_native().unwrap()).unwrap();
    hwpx.set_picture_properties_native(0, para, ctrl, r#"{"cropLeft":600}"#)
        .unwrap();
    let hwp = DocumentCore::from_bytes(&hwpx.export_hwp_with_adapter_snapshot().unwrap()).unwrap();
    let v = props(&hwp);
    assert_eq!(
        (&v["cropExtentWidth"], &v["cropLeft"], &v["cropRight"]),
        (
            &serde_json::json!(1200),
            &serde_json::json!(600),
            &serde_json::json!(0)
        )
    );
}
