//! 그림 효과(그림자·네온·반사)를 그림 뒤·위에 겹쳐 그릴 래스터로 준비한다.
//!
//! 옅은 테두리(`picture_soft_edge`)처럼 공통 paint 단계에서 만들어 Canvas·SVG·인쇄가 같은 결과를 쓴다.
//! 모양은 그림 틀(테두리가 있으면 테두리 바깥까지)의 사각형이다. 흐림 폭·짙기·변형 순서는 한컴 12.30 Mac에
//! 그림 도구 상자 견본 71개를 같은 그림에 걸어 화면에서 잰 값이다.
//! - 바깥 그림자: 사각형을 기준점에서 기울이고(원래 높이로) 늘인 뒤 방향·거리만큼 옮겨 가우스 흐림
//!   (σ = 반경×0.55), 가장 짙은 곳은 1−투명도.
//! - 안쪽 그림자: 방향 쪽 안 가장자리가 어둡다(그림 밖 영역을 반대쪽으로 옮겨 흐린 것).
//! - 네온: 틀 밖 거리 d에서 (1−투명도)·exp(−d²/2σ²), σ = 반경/4. 색 효과 SAT_MOD는 HSL 채도 곱이다.
//! - 반사: 기준점에서 뒤집은 그림을 방향·거리만큼 옮기고, 붙은 끝에서 끝 위치까지 투명도를 잇는다.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Cursor;

use super::{decode_image_with_format_limited, memoized, Conversion};
use crate::model::image::{EffectColor, EffectPoint, Picture, PictureEffects};
use crate::paint::{PaintOp, ResolvedImageKind, ResolvedImagePayload};
use crate::renderer::render_tree::{BoundingBox, ImageNode};

const SHADOW_SIGMA_PER_RADIUS: f64 = 0.55;
const GLOW_SIGMA_PER_RADIUS: f64 = 0.25;
/// 효과 래스터의 레이아웃 픽셀당 표본 수. 흐린 효과라 화면 확대에도 충분하다.
const WORKING_SCALE: f64 = 2.0;
const MAX_PIXELS: f64 = 4_000_000.0;

/// 렌더 노드가 들고 다니는 그림 효과(그림자·네온·반사)와 테두리.
#[derive(Debug, Clone)]
pub struct PictureEffectPaint {
    pub effects: PictureEffects,
    /// 테두리 선 (색 0x00BBGGRR, 굵기 HWPUNIT). 효과 모양은 테두리 바깥까지다.
    pub border: Option<(u32, i32)>,
}

impl PictureEffectPaint {
    pub fn from_picture(pic: &Picture) -> Option<Box<Self>> {
        let e = &pic.effects;
        if e.shadow.is_none() && e.glow.is_none() && e.reflection.is_none() {
            return None;
        }
        Some(Box::new(Self {
            effects: PictureEffects {
                shadow: e.shadow.clone(),
                glow: e.glow.clone(),
                soft_edge_radius: None,
                reflection: e.reflection.clone(),
            },
            border: (pic.border_attr.attr & 0x3f != 0)
                .then_some((pic.border_color, pic.border_width)),
        }))
    }
}

/// 그림 뒤(바깥 그림자·네온·반사)와 그림 위(안쪽 그림자)에 그릴 연산.
#[derive(Default)]
pub(crate) struct PictureEffectOps {
    pub behind: Vec<PaintOp>,
    pub front: Vec<PaintOp>,
}

impl PictureEffectOps {
    pub fn is_empty(&self) -> bool {
        self.behind.is_empty() && self.front.is_empty()
    }
}

pub(crate) fn prepare_picture_effects(
    image: &ImageNode,
    frame: BoundingBox,
    dpi: f64,
    reflect_source: bool,
) -> PictureEffectOps {
    let mut ops = PictureEffectOps::default();
    let Some(fx) = image.picture_effects.as_deref() else {
        return ops;
    };
    if !dpi.is_finite() || dpi <= 0.0 || image.fill_mode.is_some() {
        return ops;
    }
    let px = |hu: f64| hu * dpi / 7200.0;
    let Some(shape) = Silhouette::new(image, frame, border_outset(fx, dpi)) else {
        return ops;
    };
    let shadow = fx.effects.shadow.as_ref().map(|s| {
        let strength = 1.0 - num(s.alpha.as_deref()).clamp(0.0, 1.0);
        let sigma = px(num(s.radius.as_deref())) * SHADOW_SIGMA_PER_RADIUS;
        let offset = polar(num(s.direction.as_deref()), px(num(s.distance.as_deref())));
        (s, strength, sigma, offset, rgb(s.color.as_ref()))
    });
    let inside = |s: &&crate::model::image::PictureShadow| s.style.as_deref() == Some("INSIDE");
    // 그림 뒤는 그림자·네온·반사 차례로 쌓는다.
    if let Some((s, strength, sigma, offset, color)) = shadow.filter(|(s, ..)| !inside(s)) {
        let t = Affine::new(
            shape.page,
            s.align_style.as_deref(),
            s.skew.as_ref(),
            s.scale.as_ref(),
            offset,
        );
        ops.behind
            .extend(layer_op(outer_shadow(&shape, &t, sigma, strength, color)));
    }
    if let Some(g) = &fx.effects.glow {
        let strength = 1.0 - num(g.alpha.as_deref()).clamp(0.0, 1.0);
        let sigma = px(num(g.radius.as_deref())) * GLOW_SIGMA_PER_RADIUS;
        ops.behind.extend(layer_op(glow(
            &shape,
            sigma,
            strength,
            rgb(g.color.as_ref()),
        )));
    }
    if let Some(r) = &fx.effects.reflection {
        let source = reflect_source
            .then_some(image.data.as_deref())
            .flatten()
            .filter(|d| !d.is_empty());
        if let Some(source) = source {
            let border_rgb = fx.border.map(|(c, _)| bgr(c));
            ops.behind.extend(layer_op(reflection(
                image, source, &shape, border_rgb, r, &px,
            )));
        }
    }
    if let Some((_, strength, sigma, (dx, dy), color)) = shadow.filter(|(s, ..)| inside(s)) {
        ops.front.extend(layer_op(inner_shadow(
            &shape, sigma, strength, -dx, -dy, color,
        )));
    }
    ops
}

/// 한컴 12.30은 떠 있는 그림의 위치를 효과(그림자·네온·반사)까지 덮는 상자의 왼쪽 위로 보고,
/// 그림을 그 안쪽으로 민다. 효과 상자는 흐림 반경만큼 넓힌 그림자 모양과 반경만큼 넓힌 네온이다
/// (견본 23개 그림자·18개 네온의 화면 위치로 확인). 글자처럼 취급한 그림은 옮기지 않는다.
pub fn shift_floating_pictures_for_effects(
    tree: &mut crate::renderer::render_tree::PageRenderTree,
    dpi: f64,
) {
    use crate::renderer::render_tree::{RenderNode, RenderNodeType};
    use std::collections::HashMap;
    fn pictures(
        node: &mut RenderNode,
        dpi: f64,
        moved: &mut HashMap<crate::renderer::render_tree::NodeId, (f64, f64)>,
    ) {
        if let RenderNodeType::Image(image) = &node.node_type {
            if !node.source_inline_picture {
                let (dx, dy) = effect_origin_shift(image, node.bbox, dpi);
                if dx > 0.0 || dy > 0.0 {
                    node.bbox.x += dx;
                    node.bbox.y += dy;
                    moved.insert(node.id, (dx, dy));
                }
            }
        }
        for child in &mut node.children {
            pictures(child, dpi, moved);
        }
    }
    fn borders(
        node: &mut RenderNode,
        moved: &HashMap<crate::renderer::render_tree::NodeId, (f64, f64)>,
    ) {
        if let RenderNodeType::Rectangle(rect) = &node.node_type {
            if let Some((dx, dy)) = rect.image_frame_owner.and_then(|id| moved.get(&id)) {
                node.bbox.x += dx;
                node.bbox.y += dy;
            }
        }
        for child in &mut node.children {
            borders(child, moved);
        }
    }
    let mut moved = HashMap::new();
    pictures(&mut tree.root, dpi, &mut moved);
    if !moved.is_empty() {
        borders(&mut tree.root, &moved);
    }
}

/// 그림 틀 왼쪽 위에서 효과 상자 왼쪽 위까지의 거리(밖으로 나간 만큼, 없으면 0).
fn effect_origin_shift(image: &ImageNode, frame: BoundingBox, dpi: f64) -> (f64, f64) {
    let Some(fx) = image.picture_effects.as_deref() else {
        return (0.0, 0.0);
    };
    if !dpi.is_finite() || dpi <= 0.0 || image.fill_mode.is_some() {
        return (0.0, 0.0);
    }
    let px = |hu: f64| hu * dpi / 7200.0;
    let outset = border_outset(fx, dpi);
    let Some(shape) = Silhouette::new(image, frame, outset) else {
        return (0.0, 0.0);
    };
    let mut boxes = Vec::new();
    if let Some(s) = fx
        .effects
        .shadow
        .as_ref()
        .filter(|s| s.style.as_deref() != Some("INSIDE"))
    {
        let offset = polar(num(s.direction.as_deref()), px(num(s.distance.as_deref())));
        let t = Affine::new(
            shape.page,
            s.align_style.as_deref(),
            s.skew.as_ref(),
            s.scale.as_ref(),
            offset,
        );
        let corners = shape.outer.corners().map(|p| t.forward(apply(&shape.m, p)));
        boxes.push(bounds(corners).grow(px(num(s.radius.as_deref()))));
    }
    if let Some(g) = &fx.effects.glow {
        boxes.push(shape.page.grow(px(num(g.radius.as_deref()))));
    }
    if let Some(r) = &fx.effects.reflection {
        let offset = polar(num(r.direction.as_deref()), px(num(r.distance.as_deref())));
        let t = Affine::new(
            shape.page,
            r.align_style.as_deref(),
            r.skew.as_ref(),
            r.scale.as_ref(),
            offset,
        );
        boxes.push(bounds(shape.page.corners().map(|p| t.forward(p))));
    }
    boxes.iter().fold((0.0_f64, 0.0_f64), |(dx, dy), b| {
        (dx.max(shape.page.x - b.x), dy.max(shape.page.y - b.y))
    })
}

fn border_outset(fx: &PictureEffectPaint, dpi: f64) -> f64 {
    // 테두리는 그림 안으로 한 화면 픽셀(75HU)까지 겹치고 나머지가 바깥으로 자란다(picture_border).
    let px = |hu: f64| hu * dpi / 7200.0;
    fx.border.map_or(0.0, |(_, width)| {
        let w = if width > 0 {
            px(f64::from(width))
        } else {
            0.1 / 25.4 * dpi
        };
        w - w.min(px(75.0))
    })
}

fn layer_op(layer: Option<(BoundingBox, Vec<u8>)>) -> Option<PaintOp> {
    let (bbox, data) = layer?;
    Some(PaintOp::image(
        bbox,
        ImageNode::new(0, Some(data.clone())),
        Some(ResolvedImagePayload {
            data,
            mime: "image/png",
            kind: ResolvedImageKind::SourceWindow,
            suppress_effects: true,
        }),
    ))
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Rect {
    fn from_bbox(b: BoundingBox) -> Self {
        Self {
            x: b.x,
            y: b.y,
            w: b.width,
            h: b.height,
        }
    }

    fn grow(self, d: f64) -> Self {
        Self {
            x: self.x - d,
            y: self.y - d,
            w: self.w + 2.0 * d,
            h: self.h + 2.0 * d,
        }
    }

    fn corners(&self) -> [(f64, f64); 4] {
        [
            (self.x, self.y),
            (self.x + self.w, self.y),
            (self.x, self.y + self.h),
            (self.x + self.w, self.y + self.h),
        ]
    }

    /// 사각형 밖으로 떨어진 거리(안은 0).
    fn distance(&self, (x, y): (f64, f64)) -> f64 {
        let dx = (self.x - x).max(x - self.x - self.w).max(0.0);
        let dy = (self.y - y).max(y - self.y - self.h).max(0.0);
        dx.hypot(dy)
    }

    fn contains(&self, p: (f64, f64)) -> bool {
        self.distance(p) <= 0.0
    }
}

fn bounds(points: impl IntoIterator<Item = (f64, f64)>) -> Rect {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (x, y) in points {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    Rect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

/// 그림의 화면 모양: 회전 전 그림 사각형(로컬)과 그림이 쓰는 회전·대칭 행렬.
/// 효과의 기준점·방향·반사 축은 한컴처럼 회전한 모양의 쪽 공간 상자에서 잰다.
struct Silhouette {
    /// 테두리 바깥까지의 로컬 사각형.
    outer: Rect,
    /// 그림이 그려지는 로컬 사각형.
    picture: Rect,
    /// 로컬 → 쪽 (x' = a·x + c·y + e, y' = b·x + d·y + f).
    m: [f64; 6],
    inv: [f64; 6],
    /// 회전한 바깥 모양의 쪽 공간 상자.
    page: Rect,
}

impl Silhouette {
    fn new(image: &ImageNode, frame: BoundingBox, outset: f64) -> Option<Self> {
        let bitmap = image.transformed_paint_bbox(&frame);
        let picture = Rect::from_bbox(bitmap);
        let outer = picture.grow(outset);
        if !(outer.w > 0.0 && outer.h > 0.0) {
            return None;
        }
        let m = if image.transform.has_transform() {
            image.transform.matrix_about(bitmap)
        } else {
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
        };
        let det = m[0] * m[3] - m[1] * m[2];
        if det.abs() < 1e-9 {
            return None;
        }
        let inv_lin = [m[3] / det, -m[1] / det, -m[2] / det, m[0] / det];
        let inv = [
            inv_lin[0],
            inv_lin[1],
            inv_lin[2],
            inv_lin[3],
            -(inv_lin[0] * m[4] + inv_lin[2] * m[5]),
            -(inv_lin[1] * m[4] + inv_lin[3] * m[5]),
        ];
        let page = bounds(outer.corners().map(|p| apply(&m, p)));
        Some(Self {
            outer,
            picture,
            m,
            inv,
            page,
        })
    }

    fn local(&self, p: (f64, f64)) -> (f64, f64) {
        apply(&self.inv, p)
    }

    fn spec(&self) -> [f64; 14] {
        [
            self.outer.w,
            self.outer.h,
            self.picture.x - self.outer.x,
            self.picture.y - self.outer.y,
            self.picture.w,
            self.picture.h,
            self.m[0],
            self.m[1],
            self.m[2],
            self.m[3],
            self.page.x - self.outer.x,
            self.page.y - self.outer.y,
            self.page.w,
            self.page.h,
        ]
    }
}

fn apply(m: &[f64; 6], (x, y): (f64, f64)) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// 기준점 a에서 q = a + M(p − a) + off. M = [[sx, tan kx], [tan ky, sy]] (기울기는 원래 길이로 잰다).
struct Affine {
    ax: f64,
    ay: f64,
    m: [f64; 4],
    off: (f64, f64),
}

impl Affine {
    fn new(
        rect: Rect,
        align: Option<&str>,
        skew: Option<&EffectPoint>,
        scale: Option<&EffectPoint>,
        off: (f64, f64),
    ) -> Self {
        let align = align.unwrap_or("CENTER");
        let ax = if align.ends_with("LEFT") {
            rect.x
        } else if align.ends_with("RIGHT") {
            rect.x + rect.w
        } else {
            rect.x + rect.w / 2.0
        };
        let ay = if align.starts_with("TOP") {
            rect.y
        } else if align.starts_with("BOTTOM") {
            rect.y + rect.h
        } else {
            rect.y + rect.h / 2.0
        };
        let point = |p: Option<&EffectPoint>, default: f64| {
            p.map_or((default, default), |p| {
                (
                    p.x.as_deref().map_or(default, |v| num(Some(v))),
                    p.y.as_deref().map_or(default, |v| num(Some(v))),
                )
            })
        };
        let (kx, ky) = point(skew, 0.0);
        let (sx, sy) = point(scale, 1.0);
        Self {
            ax,
            ay,
            m: [sx, kx.to_radians().tan(), ky.to_radians().tan(), sy],
            off,
        }
    }

    fn forward(&self, (x, y): (f64, f64)) -> (f64, f64) {
        let (u, v) = (x - self.ax, y - self.ay);
        (
            self.ax + self.m[0] * u + self.m[1] * v + self.off.0,
            self.ay + self.m[2] * u + self.m[3] * v + self.off.1,
        )
    }

    fn inverse(&self, (x, y): (f64, f64)) -> Option<(f64, f64)> {
        let det = self.m[0] * self.m[3] - self.m[1] * self.m[2];
        if det.abs() < 1e-6 {
            return None;
        }
        let (u, v) = (x - self.ax - self.off.0, y - self.ay - self.off.1);
        Some((
            self.ax + (self.m[3] * u - self.m[1] * v) / det,
            self.ay + (-self.m[2] * u + self.m[0] * v) / det,
        ))
    }

    fn spec(&self, origin: Rect) -> [f64; 8] {
        [
            self.m[0],
            self.m[1],
            self.m[2],
            self.m[3],
            self.off.0,
            self.off.1,
            self.ax - origin.x,
            self.ay - origin.y,
        ]
    }
}

fn outer_shadow(
    shape: &Silhouette,
    t: &Affine,
    sigma: f64,
    strength: f64,
    color: [u8; 3],
) -> Option<(BoundingBox, Vec<u8>)> {
    if strength <= 0.0 {
        return None;
    }
    t.inverse((0.0, 0.0))?;
    // 흐림은 변형 뒤 공간에서 같은 폭이다. 늘인 축의 σ를 원래 공간으로 되돌린다.
    let sx = t.m[0].abs().max(0.01);
    let sy = t.m[3].abs().max(0.01);
    let corners = shape.outer.corners().map(|p| t.forward(apply(&shape.m, p)));
    let area = bounds(corners).grow(3.0 * sigma);
    let o = shape.outer;
    let mut spec = vec![0.0, sigma, strength, area.x - o.x, area.y - o.y];
    spec.extend(shape.spec());
    spec.extend(t.spec(o));
    raster(area, key(&spec), color, |q| {
        // 그림·테두리 아래는 비운다. 테두리는 그림보다 먼저 그려지는 형제라 덮지 않는다.
        if shape.outer.contains(shape.local(q)) {
            return Some(0.0);
        }
        let (u, v) = shape.local(t.inverse(q)?);
        Some(strength * band(u, o.x, o.x + o.w, sigma / sx) * band(v, o.y, o.y + o.h, sigma / sy))
    })
}

fn inner_shadow(
    shape: &Silhouette,
    sigma: f64,
    strength: f64,
    hole_dx: f64,
    hole_dy: f64,
    color: [u8; 3],
) -> Option<(BoundingBox, Vec<u8>)> {
    if strength <= 0.0 {
        return None;
    }
    let p = shape.picture;
    let area = bounds(p.corners().map(|c| apply(&shape.m, c)));
    let mut spec = vec![1.0, sigma, strength, hole_dx, hole_dy];
    spec.extend(shape.spec());
    raster(area, key(&spec), color, |q| {
        if !p.contains(shape.local(q)) {
            return Some(0.0);
        }
        let (u, v) = shape.local((q.0 - hole_dx, q.1 - hole_dy));
        let lit = band(u, p.x, p.x + p.w, sigma) * band(v, p.y, p.y + p.h, sigma);
        Some(strength * (1.0 - lit))
    })
}

fn glow(
    shape: &Silhouette,
    sigma: f64,
    strength: f64,
    color: [u8; 3],
) -> Option<(BoundingBox, Vec<u8>)> {
    if strength <= 0.0 || sigma <= 0.0 {
        return None;
    }
    let area = shape.page.grow(3.0 * sigma);
    let mut spec = vec![2.0, sigma, strength];
    spec.extend(shape.spec());
    raster(area, key(&spec), color, |q| {
        // 모양 안(d = 0)은 그림·테두리가 덮으므로 비운다.
        let d = shape.outer.distance(shape.local(q));
        Some(if d > 0.0 {
            strength * (-d * d / (2.0 * sigma * sigma)).exp()
        } else {
            0.0
        })
    })
}

fn reflection(
    image: &ImageNode,
    source: &[u8],
    shape: &Silhouette,
    border: Option<[u8; 3]>,
    r: &crate::model::image::PictureReflection,
    px: &dyn Fn(f64) -> f64,
) -> Option<(BoundingBox, Vec<u8>)> {
    let offset = polar(num(r.direction.as_deref()), px(num(r.distance.as_deref())));
    let t = Affine::new(
        shape.page,
        r.align_style.as_deref(),
        r.skew.as_ref(),
        r.scale.as_ref(),
        offset,
    );
    let range = |v: Option<&crate::model::image::EffectRange>| {
        (
            num(v.and_then(|v| v.start.as_deref())),
            num(v.and_then(|v| v.end.as_deref())),
        )
    };
    let (alpha0, alpha1) = range(r.alpha.as_ref());
    let (pos0, pos1) = range(r.pos.as_ref());
    if pos1 <= pos0 || t.inverse((0.0, 0.0)).is_none() {
        return None;
    }
    let format = image::guess_format(source).ok()?;
    let header =
        super::canvaskit_encoded_image_header(source).filter(|h| h.is_within_decode_limits())?;
    let crop = image.crop.map_or(
        (0.0, 0.0, f64::from(header.width), f64::from(header.height)),
        |c| {
            crate::renderer::svg::compute_image_crop_src(
                c,
                super::image_crop_reference(image),
                f64::from(header.width),
                f64::from(header.height),
            )
        },
    );
    let pic = shape.picture;
    let (sw, sh) = (
        (pic.w * WORKING_SCALE).round().max(1.0) as u32,
        (pic.h * WORKING_SCALE).round().max(1.0) as u32,
    );
    if f64::from(sw) * f64::from(sh) > MAX_PIXELS {
        return None;
    }
    // 반사 길이는 기준점(붙은 끝)에서 회전한 모양 높이에 대해 잰다.
    let page = shape.page;
    let area = bounds(page.corners().map(|p| t.forward(p)));
    let (w, h, scale) = size(area)?;
    let opacity = image.opacity.clamp(0.0, 1.0);
    let mut spec = vec![
        3.0,
        area.x - page.x,
        area.y - page.y,
        alpha0,
        alpha1,
        pos0,
        pos1,
        opacity,
        crop.0,
        crop.1,
        crop.2,
        crop.3,
        border.map_or(-1.0, |c| {
            f64::from(u32::from_be_bytes([0, c[0], c[1], c[2]]))
        }),
    ];
    spec.extend(shape.spec());
    spec.extend(t.spec(page));
    let spec = key(&spec);
    let bytes = memoized(Conversion::PictureEffect { spec }, source, || {
        let decoded = decode_image_with_format_limited(source, format)?.to_rgba8();
        let pixels = super::picture_soft_edge::sample_window(&decoded, crop, sw, sh);
        let mut out = image::RgbaImage::new(w, h);
        for (ix, iy, pixel) in out.enumerate_pixels_mut() {
            let q = (
                area.x + (f64::from(ix) + 0.5) / scale,
                area.y + (f64::from(iy) + 0.5) / scale,
            );
            let Some(p) = t.inverse(q) else {
                continue;
            };
            let along = ((p.1 - t.ay) / page.h).abs();
            if along > pos1 {
                continue;
            }
            let (u, v) = shape.local(p);
            if !shape.outer.contains((u, v)) {
                continue;
            }
            let k = ((along - pos0) / (pos1 - pos0)).clamp(0.0, 1.0);
            let fade = (1.0 - (alpha0 + (alpha1 - alpha0) * k)).clamp(0.0, 1.0) * opacity;
            let (cx, cy) = (
                ((u - pic.x) * WORKING_SCALE).floor(),
                ((v - pic.y) * WORKING_SCALE).floor(),
            );
            let rgba = if cx >= 0.0 && cy >= 0.0 && cx < f64::from(sw) && cy < f64::from(sh) {
                pixels.get_pixel(cx as u32, cy as u32).0
            } else if let Some(c) = border {
                [c[0], c[1], c[2], 255]
            } else {
                continue;
            };
            *pixel = image::Rgba([
                rgba[0],
                rgba[1],
                rgba[2],
                (f64::from(rgba[3]) * fade).round() as u8,
            ]);
        }
        encode(out)
    })?;
    Some((bbox_of(area, w, h, scale), bytes))
}

/// 효과 영역을 표본 격자로 채운다. 값은 그 점의 효과 짙기(0~1)다.
fn raster(
    area: Rect,
    spec: u64,
    color: [u8; 3],
    alpha: impl Fn((f64, f64)) -> Option<f64>,
) -> Option<(BoundingBox, Vec<u8>)> {
    let (w, h, scale) = size(area)?;
    let spec = {
        let mut hasher = DefaultHasher::new();
        (spec, color).hash(&mut hasher);
        hasher.finish()
    };
    let bytes = memoized(Conversion::PictureEffect { spec }, &[], || {
        let mut out = image::RgbaImage::new(w, h);
        for (ix, iy, pixel) in out.enumerate_pixels_mut() {
            let x = area.x + (f64::from(ix) + 0.5) / scale;
            let y = area.y + (f64::from(iy) + 0.5) / scale;
            let a = alpha((x, y)).unwrap_or(0.0).clamp(0.0, 1.0);
            *pixel = image::Rgba([color[0], color[1], color[2], (a * 255.0).round() as u8]);
        }
        encode(out)
    })?;
    Some((bbox_of(area, w, h, scale), bytes))
}

fn size(area: Rect) -> Option<(u32, u32, f64)> {
    if !(area.w.is_finite() && area.h.is_finite()) || area.w <= 0.0 || area.h <= 0.0 {
        return None;
    }
    let scale = WORKING_SCALE.min((MAX_PIXELS / (area.w * area.h)).sqrt());
    let w = (area.w * scale).ceil().max(1.0) as u32;
    let h = (area.h * scale).ceil().max(1.0) as u32;
    Some((w, h, scale))
}

fn bbox_of(area: Rect, w: u32, h: u32, scale: f64) -> BoundingBox {
    BoundingBox::new(area.x, area.y, f64::from(w) / scale, f64::from(h) / scale)
}

fn encode(pixels: image::RgbaImage) -> Option<Vec<u8>> {
    let mut bytes = Cursor::new(Vec::new());
    pixels.write_to(&mut bytes, image::ImageFormat::Png).ok()?;
    Some(bytes.into_inner())
}

fn key(values: &[f64]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for v in values {
        // 위치는 1/1024 단위로 묶어 스크롤·부동소수 흔들림에 같은 키를 쓴다.
        ((v * 1024.0).round() as i64).hash(&mut hasher);
    }
    hasher.finish()
}

/// 1차원 상자[lo, hi]를 σ로 흐린 값.
fn band(t: f64, lo: f64, hi: f64, sigma: f64) -> f64 {
    if sigma <= 1e-6 {
        return f64::from(u8::from(t >= lo && t <= hi));
    }
    phi((t - lo) / sigma) - phi((t - hi) / sigma)
}

/// 표준 정규 누적 분포(Abramowitz–Stegun 7.1.26, 오차 1.5e-7).
fn phi(z: f64) -> f64 {
    let x = z.abs() / std::f64::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736
                + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let erf = 1.0 - poly * (-x * x).exp();
    0.5 * (1.0 + erf.copysign(z))
}

/// 방향(도, 0=오른쪽·90=아래)과 거리를 화면 이동으로.
fn polar(direction: f64, distance: f64) -> (f64, f64) {
    let r = direction.to_radians();
    (r.cos() * distance, r.sin() * distance)
}

fn num(v: Option<&str>) -> f64 {
    v.and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
}

fn bgr(c: u32) -> [u8; 3] {
    [
        (c & 0xff) as u8,
        ((c >> 8) & 0xff) as u8,
        ((c >> 16) & 0xff) as u8,
    ]
}

/// 효과 색(RGB)과 색 효과. SAT_MOD는 HSL 채도에 곱한다(1을 넘으면 1).
fn rgb(color: Option<&EffectColor>) -> [u8; 3] {
    let Some(color) = color else {
        return [0, 0, 0];
    };
    let channel = |v: Option<&String>| num(v.map(String::as_str)).clamp(0.0, 255.0);
    let rgb = color.rgb.as_ref();
    let mut c = [
        channel(rgb.and_then(|c| c.r.as_ref())),
        channel(rgb.and_then(|c| c.g.as_ref())),
        channel(rgb.and_then(|c| c.b.as_ref())),
    ];
    for effect in &color.effects {
        if effect.kind == "SAT_MOD" {
            c = saturate(c, num(Some(&effect.value)));
        }
    }
    c.map(|v| v.round().clamp(0.0, 255.0) as u8)
}

fn saturate(c: [f64; 3], factor: f64) -> [f64; 3] {
    let [r, g, b] = c.map(|v| v / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d <= 0.0 {
        return c;
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    let s = (s * factor).clamp(0.0, 1.0);
    let chroma = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = chroma * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = match h as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = l - chroma / 2.0;
    [r1 + m, g1 + m, b1 + m].map(|v| v * 255.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blurred_box_is_half_at_edge_and_full_inside() {
        assert!((band(0.0, 0.0, 100.0, 5.0) - 0.5).abs() < 1e-6);
        assert!(band(50.0, 0.0, 100.0, 5.0) > 0.999);
        assert!(band(-20.0, 0.0, 100.0, 5.0) < 1e-4);
    }

    #[test]
    fn perspective_skew_uses_original_height() {
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 80.0,
        };
        let skew = EffectPoint {
            x: Some("25".into()),
            y: Some("0".into()),
        };
        let scale = EffectPoint {
            x: Some("1".into()),
            y: Some("0.25".into()),
        };
        let t = Affine::new(
            rect,
            Some("BOTTOM_RIGHT"),
            Some(&skew),
            Some(&scale),
            (0.0, 0.0),
        );
        let (x, y) = t.forward((0.0, 0.0));
        assert!((y - 60.0).abs() < 1e-9);
        assert!((x + 80.0 * 25f64.to_radians().tan()).abs() < 1e-9);
        let (u, v) = t.inverse((x, y)).unwrap();
        assert!(u.abs() < 1e-9 && v.abs() < 1e-9);
    }

    #[test]
    fn rotated_silhouette_maps_page_points_back_to_the_picture() {
        let mut image = ImageNode::new(0, None);
        image.transform.rotation = 30.0;
        let frame = BoundingBox::new(0.0, 0.0, 100.0, 40.0);
        let shape = Silhouette::new(&image, frame, 0.0).unwrap();
        // 30° 돌린 100×40 모양의 쪽 상자는 106.6×84.6이다.
        assert!((shape.page.w - 106.60).abs() < 0.01 && (shape.page.h - 84.64).abs() < 0.01);
        let center = (
            shape.page.x + shape.page.w / 2.0,
            shape.page.y + shape.page.h / 2.0,
        );
        let (u, v) = shape.local(center);
        assert!((u - 50.0).abs() < 1e-6 && (v - 20.0).abs() < 1e-6);
    }

    #[test]
    fn sat_mod_keeps_grey_and_saturates_colour() {
        assert_eq!(saturate([178.0; 3], 1.75), [178.0; 3]);
        let [r, g, b] = saturate([97.0, 130.0, 214.0], 1.75);
        assert!(b > 214.0 && r < 97.0 && g < 130.0);
    }
}
