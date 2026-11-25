//! 한/글 12.30은 그림자를 건 도형을 HWP로 저장하면 그림자가 틀 밖으로 나가는 몫을 개체 바깥 여백 칸에
//! 더해 적고, HWPX `outMargin`에는 더하지 않는다. 기울인 도형은 그림자를 기울인 외곽에서 다시 잰다.
//! 표본은 `shape_skew/hancom_skew.hwpx`의 사각형에 그림자를 걸어 한/글 12.30(Mac)에서 열고 HWP·HWPX로
//! 다시 저장한 문서다(`*.hwp`는 한/글 저장본, `hancom_shadow.hwpx`도 한/글 저장본).
#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;

use rhwp::document_core::DocumentCore;
use rhwp::paint::{GroupKind, LayerBuilder, LayerNode, LayerNodeKind, RenderProfile};
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};

fn read(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/shape_shadow")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("fixture 누락 {name}: {e}"))
}

fn records(buf: &[u8]) -> Vec<(u16, Vec<u8>)> {
    let mut out = Vec::new();
    let mut off = 0usize;
    while off + 4 <= buf.len() {
        let h = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap());
        off += 4;
        let mut size = (h >> 20) as usize;
        if size == 0xFFF {
            size = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap()) as usize;
            off += 4;
        }
        let end = (off + size).min(buf.len());
        out.push(((h & 0x3FF) as u16, buf[off..end].to_vec()));
        off = end;
    }
    out
}

/// HWP 첫 구역에 적힌 그리기 개체 CTRL_HEADER의 (가로 offset, 세로 offset, 바깥 여백 좌·우·상·하).
fn gso_margins(hwp: &[u8]) -> Vec<(u32, u32, [i16; 4])> {
    const CTRL_HEADER: u16 = 0x010 + 55;
    let mut cfb = cfb::CompoundFile::open(std::io::Cursor::new(hwp.to_vec())).expect("CFB 열기");
    let mut read = |path: &str| {
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut cfb.open_stream(path).expect(path), &mut buf).unwrap();
        buf
    };
    let compressed = read("/FileHeader")[36] & 1 == 1;
    let raw = read("/BodyText/Section0");
    let body = if compressed {
        let mut out = Vec::new();
        std::io::Read::read_to_end(&mut flate2::read::DeflateDecoder::new(&raw[..]), &mut out)
            .unwrap();
        out
    } else {
        raw
    };
    records(&body)
        .into_iter()
        .filter(|(tag, data)| *tag == CTRL_HEADER && data.starts_with(b" osg"))
        .map(|(_, d)| {
            let i16_at = |o: usize| i16::from_le_bytes([d[o], d[o + 1]]);
            let u32_at = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
            (
                u32_at(12),
                u32_at(8),
                [i16_at(28), i16_at(30), i16_at(32), i16_at(34)],
            )
        })
        .collect()
}

#[test]
fn hwp_save_adds_shadow_extents_to_outer_margin_like_hancom() {
    // 평행·기울임·작게·크게 그림자, 여백 500, 기울인 사각형의 그림자를 한/글 저장본과 같은 값으로 적는다.
    for (source, hancom) in [
        ("hancom_shadow.hwpx", "hancom_shadow.hwp"),
        ("shear_scale.hwpx", "shear_scale.hwp"),
        ("skewed.hwpx", "skewed.hwp"),
    ] {
        let core = DocumentCore::from_bytes(&read(source)).expect("HWPX 열기");
        let ours = gso_margins(&core.export_hwp_native().expect("HWP 저장"));
        assert_eq!(ours, gso_margins(&read(hancom)), "{source}");
    }
}

#[test]
fn hancom_hwp_shadow_margin_reads_clean_and_round_trips() {
    let core = DocumentCore::from_bytes(&read("hancom_shadow.hwp")).expect("HWP 열기");
    let paragraph = &core.document().sections[0].paragraphs[0];
    let shapes: Vec<usize> = paragraph
        .controls
        .iter()
        .enumerate()
        .filter(|(_, c)| matches!(c, rhwp::model::control::Control::Shape(_)))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(shapes.len(), 8);
    // 개체 속성 창의 바깥 여백은 한/글 HWPX와 같다(그림자 몫을 뺀 값).
    for (ctrl, margin) in shapes.iter().zip([0, 0, 0, 0, 500, 0, 0, 0]) {
        let props = core
            .get_shape_properties_native(0, 0, *ctrl)
            .expect("props");
        for key in ["Left", "Right", "Top", "Bottom"] {
            assert!(
                props.contains(&format!("\"outerMargin{key}\":{margin},")),
                "{ctrl}: {props}"
            );
        }
    }
    // HWP로 다시 저장하면 한/글 저장본의 여백을 그대로, HWPX로 저장하면 그림자 몫 없이 적는다.
    let hancom = gso_margins(&read("hancom_shadow.hwp"));
    assert_eq!(gso_margins(&core.export_hwp_native().unwrap()), hancom);
    let hwpx = DocumentCore::from_bytes(&core.export_hwpx_native().unwrap()).expect("HWPX 열기");
    assert_eq!(gso_margins(&hwpx.export_hwp_native().unwrap()), hancom);
}

#[test]
fn changing_shadow_rewrites_hwp_margin_from_the_new_shadow() {
    let mut core = DocumentCore::from_bytes(&read("hancom_shadow.hwp")).expect("HWP 열기");
    let first = core.document().sections[0].paragraphs[0]
        .controls
        .iter()
        .position(|c| matches!(c, rhwp::model::control::Control::Shape(_)))
        .unwrap();
    // 오른쪽 아래 평행(883) → 왼쪽 위 평행, 저장 offset 0: 기본 이동량 600만 왼쪽·위에 적는다.
    core.set_shape_properties_native(
        0,
        0,
        first,
        r#"{"shadowType":1,"shadowOffsetX":0,"shadowOffsetY":0}"#,
    )
    .unwrap();
    let margins = gso_margins(&core.export_hwp_native().unwrap());
    assert_eq!(margins[0].2, [600, 0, 600, 0]);
}

fn rect_frames(node: &RenderNode, out: &mut Vec<(f64, f64)>) {
    if matches!(node.node_type, RenderNodeType::Rectangle(_)) && node.bbox.width < 300.0 {
        out.push((node.bbox.x, node.bbox.y));
    }
    node.children.iter().for_each(|c| rect_frames(c, out));
}

/// (그림자 행렬, 그 안의 기울이기 행렬) 짝
fn shadow_over_skew(node: &LayerNode, out: &mut Vec<([f64; 6], [f64; 6])>) {
    let transform = |node: &LayerNode| match &node.kind {
        LayerNodeKind::Group {
            group_kind: GroupKind::Group(group),
            ..
        } => group.paint_transform,
        _ => None,
    };
    match &node.kind {
        LayerNodeKind::Group { children, .. } => {
            if let (Some(outer), [child]) = (transform(node), children.as_slice()) {
                if let Some(inner) = transform(child) {
                    out.push((outer, inner));
                }
            }
            children.iter().for_each(|c| shadow_over_skew(c, out));
        }
        LayerNodeKind::ClipRect { child, .. } | LayerNodeKind::ClipPath { child, .. } => {
            shadow_over_skew(child, out)
        }
        LayerNodeKind::Leaf { .. } => {}
    }
}

const HU: f64 = 96.0 / 7200.0;

#[test]
fn skewed_shape_shadow_is_measured_from_the_skewed_outline() {
    let core = DocumentCore::from_bytes(&read("skewed.hwpx")).expect("HWPX 열기");
    let tree = core.build_page_render_tree(0).expect("page");
    let mut frames = Vec::new();
    rect_frames(&tree.root, &mut frames);
    // 왼쪽 위 기울임 그림자(30000, 30000): 한/글 HWP 왼쪽 여백 1628만큼만 본체를 민다(기울이기 전 상자로 재면 4540).
    let expected = (31628.0 * HU, 30000.0 * HU);
    assert!(
        frames
            .iter()
            .any(|f| (f.0 - expected.0).abs() < 0.05 && (f.1 - expected.1).abs() < 0.05),
        "{frames:?}"
    );
    // 평행 그림자(1000, 1000 + 기본 600)는 기울인 실루엣을 그대로 옮긴다(이동량을 기울이지 않는다).
    let layers = LayerBuilder::new(RenderProfile::Screen).build(&tree);
    let mut pairs = Vec::new();
    shadow_over_skew(&layers.root, &mut pairs);
    let shift = 1600.0 * HU;
    assert!(
        pairs.iter().any(|(outer, inner)| {
            outer[..4] == [1.0, 0.0, 0.0, 1.0]
                && (outer[4] - shift).abs() < 1e-6
                && (outer[5] - shift).abs() < 1e-6
                && inner[2].abs() > 0.1
        }),
        "{pairs:?}"
    );
}

#[test]
fn rotated_line_reports_its_frame_size_before_rotation() {
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/shape_skew/hancom_skew.hwpx"),
    )
    .expect("fixture");
    let mut core = DocumentCore::from_bytes(&bytes).expect("open");
    let line = core.document().sections[0].paragraphs[0]
        .controls
        .iter()
        .position(|c| {
            matches!(c, rhwp::model::control::Control::Shape(s)
                if matches!(**s, rhwp::model::shape::ShapeObject::Line(_)))
        })
        .expect("직선");
    core.set_shape_properties_native(0, 0, line, r#"{"rotationAngle":30}"#)
        .expect("rotate");
    let props = core.get_shape_properties_native(0, 0, line).expect("props");
    // 한/글 12.30 배치 탭은 돌린 직선의 크기 칸에 원래 크기(12000×8000)를 보인다. 틀은 외접 상자다.
    for part in [
        "\"width\":14392,",
        "\"height\":12928,",
        "\"frameWidth\":12000,",
        "\"frameHeight\":8000",
    ] {
        assert!(props.contains(part), "{part}: {props}");
    }
}
