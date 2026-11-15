//! 그림 효과(그림자·네온·반사)를 화면·인쇄의 공통 paint가 그린다.
//!
//! 한컴 12.30 Mac은 그림 도구 상자 견본으로 건 그림자·네온·반사를 화면과 인쇄에 그린다. 바깥 그림자·네온·반사는
//! 그림 뒤, 안쪽 그림자는 그림 위의 래스터다. 흐림 폭과 짙기는 견본 71개를 한컴 화면에서 잰 값이다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::paint::{LayerNode, LayerNodeKind, PaintOp, RenderProfile};
use rhwp::renderer::render_tree::BoundingBox;
use rhwp::wasm_api::HwpDocument;

/// 불투명한 32×32 PNG.
fn png() -> Vec<u8> {
    let pixels = image::RgbaImage::from_pixel(32, 32, image::Rgba([200, 60, 40, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    pixels
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

/// 32×32px(2400HU) 그림 하나와 그 효과.
fn doc_with(effects: &str) -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_picture_native(
        0,
        0,
        0,
        &[],
        &png(),
        2400,
        2400,
        32,
        32,
        "png",
        "",
        None,
        None,
    )
    .unwrap();
    let (para, ctrl) = doc.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .find_map(|(pi, p)| {
            p.controls
                .iter()
                .position(|c| matches!(c, rhwp::model::control::Control::Picture(_)))
                .map(|ci| (pi, ci))
        })
        .unwrap();
    doc.set_picture_effects_native(0, para, &[], ctrl, effects)
        .unwrap();
    doc
}

/// 쪽의 그림 연산을 그리는 차례대로: (틀, PNG 바이트).
fn images(doc: &HwpDocument, profile: RenderProfile) -> Vec<(BoundingBox, Vec<u8>)> {
    fn walk(node: &LayerNode, out: &mut Vec<(BoundingBox, Vec<u8>)>) {
        match &node.kind {
            LayerNodeKind::Group { children, .. } => children.iter().for_each(|c| walk(c, out)),
            LayerNodeKind::ClipRect { child, .. } | LayerNodeKind::ClipPath { child, .. } => {
                walk(child, out)
            }
            LayerNodeKind::Leaf { ops } => {
                for op in ops {
                    if let PaintOp::Image {
                        bbox,
                        image,
                        resolved,
                    } = op
                    {
                        let bytes = resolved.as_ref().map_or_else(
                            || image.data.clone().unwrap_or_default(),
                            |p| p.data.clone(),
                        );
                        out.push((*bbox, bytes));
                    }
                }
            }
        }
    }
    let tree = doc.build_page_layer_tree_with_profile(0, profile).unwrap();
    let mut out = Vec::new();
    walk(&tree.root, &mut out);
    out
}

fn alpha_at(png: &[u8], bbox: BoundingBox, x: f64, y: f64) -> u8 {
    let img = image::load_from_memory(png).unwrap().to_rgba8();
    let px = ((x - bbox.x) / bbox.width * f64::from(img.width())) as u32;
    let py = ((y - bbox.y) / bbox.height * f64::from(img.height())) as u32;
    img.get_pixel(px.min(img.width() - 1), py.min(img.height() - 1))[3]
}

const OUTER_SHADOW: &str = r##"{"shadow":{"style":"OUTSIDE","alpha":0.5,"radius":600,"direction":45,"distance":600,"alignStyle":"CENTER","skewX":0,"skewY":0,"scaleX":1,"scaleY":1,"color":"#000000"}}"##;

#[test]
fn outer_shadow_is_drawn_behind_the_picture_offset_toward_its_direction() {
    for profile in [RenderProfile::Screen, RenderProfile::Print] {
        let doc = doc_with(OUTER_SHADOW);
        let list = images(&doc, profile);
        assert_eq!(list.len(), 2, "그림자와 그림({profile:?})");
        let (shadow, picture) = (&list[0], &list[1]);
        let pic = picture.0;
        // 600HU = 8px을 45°로 옮기고 흐림만큼 넓다.
        assert!(shadow.0.x + shadow.0.width > pic.x + pic.width + 8.0);
        assert!(shadow.0.y + shadow.0.height > pic.y + pic.height + 8.0);
        // 옮긴 모양 가운데는 반투명 검정(1−투명도), 반대쪽 바깥은 비었다.
        let shift = 8.0 * std::f64::consts::FRAC_1_SQRT_2;
        // 그림 오른쪽 2px: 옮긴 모양 안쪽 3.66px이라 (1−투명도)·Φ(3.66/σ) ≈ 0.4.
        let edge = alpha_at(
            &shadow.1,
            shadow.0,
            pic.x + pic.width + 2.0,
            pic.y + pic.height / 2.0,
        );
        assert!((90..=115).contains(&edge), "그림자 짙기 {edge}");
        // 그림 아래는 비운다(테두리를 덮지 않는다).
        let under = alpha_at(
            &shadow.1,
            shadow.0,
            pic.x + pic.width / 2.0 + shift,
            pic.y + pic.height / 2.0 + shift,
        );
        assert_eq!(under, 0, "그림 아래");
        let empty = alpha_at(&shadow.1, shadow.0, shadow.0.x + 1.0, shadow.0.y + 1.0);
        assert!(empty < 5, "왼쪽 위 바깥 {empty}");
    }
}

#[test]
fn glow_reflection_and_inner_shadow_keep_hancom_order() {
    let doc = doc_with(
        r##"{"shadow":{"style":"INSIDE","alpha":0.5,"radius":600,"direction":180,"distance":600,"alignStyle":"TOP_LEFT","skewX":0,"skewY":0,"scaleX":0,"scaleY":0,"color":"#000000"},
            "glow":{"alpha":0.5,"radius":1500,"color":"#b2b2b2"},
            "reflection":{"alignStyle":"BOTTOM_LEFT","radius":50,"direction":90,"distance":400,"fadeDirection":90,"skewX":0,"skewY":0,"scaleX":1,"scaleY":-1,"startAlpha":0.5,"endAlpha":0.997,"startPos":0,"endPos":0.5}}"##,
    );
    let list = images(&doc, RenderProfile::Screen);
    assert_eq!(list.len(), 4, "네온·반사·그림·안쪽 그림자");
    let pic = list[2].0;
    let (glow, reflection, inner) = (&list[0], &list[1], &list[3]);
    // 네온: 틀 바로 밖은 1−투명도, 반경(20px)만큼 떨어지면 거의 없다.
    let edge = alpha_at(
        &glow.1,
        glow.0,
        pic.x + pic.width + 0.5,
        pic.y + pic.height / 2.0,
    );
    assert!((115..=130).contains(&edge), "네온 가장자리 {edge}");
    let far = alpha_at(
        &glow.1,
        glow.0,
        pic.x + pic.width + 14.0,
        pic.y + pic.height / 2.0,
    );
    assert!(far < 15, "네온 끝 {far}");
    // 반사: 그림 아래 4pt(≈5.3px) 떨어져 그림 높이의 절반만 보인다.
    assert!((reflection.0.y - (pic.y + pic.height + 5.33)).abs() < 1.0);
    let top = alpha_at(
        &reflection.1,
        reflection.0,
        pic.x + pic.width / 2.0,
        reflection.0.y + 0.5,
    );
    let past = alpha_at(
        &reflection.1,
        reflection.0,
        pic.x + pic.width / 2.0,
        reflection.0.y + pic.height * 0.75,
    );
    assert!(top > 90 && past == 0, "반사 짙기 {top}, 끝 뒤 {past}");
    // 안쪽 그림자: 그림 틀 안, 방향(왼쪽) 가장자리가 어둡고 반대쪽은 맑다.
    assert_eq!((inner.0.x, inner.0.y), (pic.x, pic.y));
    let left = alpha_at(&inner.1, inner.0, pic.x + 0.5, pic.y + pic.height / 2.0);
    let right = alpha_at(
        &inner.1,
        inner.0,
        pic.x + pic.width - 0.5,
        pic.y + pic.height / 2.0,
    );
    assert!(
        left > 90 && right < 10,
        "안쪽 그림자 왼쪽 {left}, 오른쪽 {right}"
    );
}

#[test]
fn removing_effects_leaves_only_the_picture() {
    let mut doc = doc_with(OUTER_SHADOW);
    let found = doc.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .find_map(|(pi, p)| {
            p.controls
                .iter()
                .position(|c| matches!(c, rhwp::model::control::Control::Picture(_)))
                .map(|ci| (pi, ci))
        })
        .unwrap();
    doc.set_picture_effects_native(0, found.0, &[], found.1, r#"{"shadow":null}"#)
        .unwrap();
    assert_eq!(images(&doc, RenderProfile::Screen).len(), 1);
}

/// 쪽 렌더 트리에서 첫 그림 틀.
fn picture_frame(doc: &HwpDocument) -> BoundingBox {
    use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};
    fn find(node: &RenderNode) -> Option<BoundingBox> {
        if matches!(node.node_type, RenderNodeType::Image(_)) {
            return Some(node.bbox);
        }
        node.children.iter().find_map(find)
    }
    find(&doc.build_page_render_tree(0).unwrap().root).unwrap()
}

#[test]
fn floating_picture_moves_inside_its_effect_box_like_hancom() {
    let plain = picture_frame(&doc_with(r#"{"shadow":null}"#));
    // 네온 15pt(20px): 효과 상자가 사방 20px 넓어지고 그림은 그만큼 안쪽에 그린다.
    let glow = picture_frame(&doc_with(
        r##"{"glow":{"alpha":0.5,"radius":1500,"color":"#b2b2b2"}}"##,
    ));
    assert!((glow.x - plain.x - 20.0).abs() < 0.01 && (glow.y - plain.y - 20.0).abs() < 0.01);
    // 오른쪽 아래 그림자: 흐림 반경 8px에서 옮긴 5.66px을 뺀 만큼만 민다.
    let shadow = picture_frame(&doc_with(OUTER_SHADOW));
    let expected = 8.0 - 8.0 * std::f64::consts::FRAC_1_SQRT_2;
    assert!(
        (shadow.x - plain.x - expected).abs() < 0.01,
        "{shadow:?} {plain:?}"
    );
    assert!((shadow.y - plain.y - expected).abs() < 0.01);
}
