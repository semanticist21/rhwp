//! 맞붙은 칸 배경은 장치 픽셀 격자에 맞춰 칠해져야 경계에 밝은 틈이 남지 않는다.
//!
//! 칸마다 배경을 따로 칠하므로 경계가 픽셀 중간에 걸리면 양쪽 안티앨리어싱 반투명
//! 픽셀이 겹쳐 밝은 세로줄이 보였다(서식 문서 회색 머리 행). 웹 캔버스는 AA 를 끌 수
//! 없어 칸 배경과 칸 clip 을 장치 픽셀에 맞춘다 — 맞붙은 두 칸은 같은 경계로 맞아야 한다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::style::BorderFill;
use rhwp::renderer::render_tree::{BoundingBox, RenderNode, RenderNodeType};
use rhwp::DocumentCore;

fn right(bbox: BoundingBox) -> f64 {
    bbox.x + bbox.width
}

#[test]
fn adjacent_boxes_snap_to_the_same_device_pixel_edge() {
    let scale = 1.5625;
    let left = BoundingBox::new(66.12, 220.41, 80.04, 30.24).snapped_to_device_pixels(scale);
    let next = BoundingBox::new(146.16, 220.41, 80.04, 30.24).snapped_to_device_pixels(scale);
    assert_eq!(right(left), next.x, "맞붙은 칸은 같은 경계를 공유해야 한다");
    for edge in [left.x, right(left), left.y, left.y + left.height] {
        let device = edge * scale;
        assert!(
            (device - device.round()).abs() < 1e-9,
            "경계 {edge} 는 장치 픽셀 위에 있어야 한다"
        );
    }
    let unchanged = BoundingBox::new(1.3, 2.7, 3.0, 4.0).snapped_to_device_pixels(0.0);
    assert_eq!((unchanged.x, unchanged.width), (1.3, 3.0));
}

fn find_rectangles(node: &RenderNode, out: &mut Vec<bool>) {
    if let RenderNodeType::Rectangle(rect) = &node.node_type {
        out.push(rect.pixel_aligned);
    }
    for child in &node.children {
        find_rectangles(child, out);
    }
}

#[test]
fn cell_background_fill_is_marked_pixel_aligned() {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.create_table_native(0, 0, 0, 1, 2).unwrap();
    let mut doc = core.document().clone();
    let mut fill = BorderFill::default();
    fill.fill.fill_type = rhwp::model::style::FillType::Solid;
    fill.fill.solid = Some(rhwp::model::style::SolidFill {
        background_color: 0x00BB_BBBB,
        pattern_color: 0,
        pattern_type: 0,
    });
    doc.doc_info.border_fills.push(fill);
    let id = doc.doc_info.border_fills.len() as u16;
    for para in &mut doc.sections[0].paragraphs {
        for control in &mut para.controls {
            if let rhwp::model::control::Control::Table(table) = control {
                for cell in &mut table.cells {
                    cell.border_fill_id = id;
                }
            }
        }
    }
    core.set_document(doc);
    let tree = core.build_page_render_tree(0).unwrap();
    let mut rects = Vec::new();
    find_rectangles(&tree.root, &mut rects);
    assert_eq!(rects, vec![true, true], "두 칸 배경 모두 픽셀 정렬 대상");
}

/// Skia 는 칸 배경과 칸 clip 을 AA 없이 칠해 맞붙은 칸 경계(x=2.5)에 틈이 없다.
#[cfg(feature = "native-skia")]
#[test]
fn skia_adjacent_cell_backgrounds_leave_no_seam_at_fractional_edges() {
    use rhwp::paint::{CacheHint, ClipKind, GroupKind, LayerNode, PageLayerTree, PaintOp};
    use rhwp::renderer::layer_renderer::RasterRenderOptions;
    use rhwp::renderer::render_tree::RectangleNode;
    use rhwp::renderer::skia::SkiaLayerRenderer;
    use rhwp::renderer::ShapeStyle;

    let cell = |x: f64, w: f64| {
        let bbox = BoundingBox::new(x, 0.0, w, 4.0);
        let mut rect = RectangleNode::new(
            0.0,
            ShapeStyle {
                fill_color: Some(0x00BB_BBBB),
                ..Default::default()
            },
            None,
        );
        rect.pixel_aligned = true;
        LayerNode::clip_rect(
            bbox,
            None,
            bbox,
            LayerNode::leaf(bbox, None, vec![PaintOp::rectangle(bbox, rect)]),
            ClipKind::TableCell,
        )
    };
    let tree = PageLayerTree::new(
        6.0,
        4.0,
        LayerNode::group(
            BoundingBox::new(0.0, 0.0, 6.0, 4.0),
            None,
            vec![cell(0.0, 2.5), cell(2.5, 3.5)],
            CacheHint::None,
            GroupKind::Generic,
        ),
    );
    let image = SkiaLayerRenderer::new()
        .render_pixels_with_options(&tree, RasterRenderOptions::default())
        .expect("render adjacent cell backgrounds");
    let pixel = |x: usize| {
        let at = 2 * image.stride + x * 4;
        image.bytes[at..at + 4].to_vec()
    };
    assert_eq!(pixel(2), pixel(0), "경계 픽셀이 왼쪽 칸과 같아야 한다");
    assert_eq!(pixel(2), pixel(4), "경계 픽셀이 오른쪽 칸과 같아야 한다");
}
