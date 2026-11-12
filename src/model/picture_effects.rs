//! HWP5 그림 꼬리의 그림 효과 정보(표 108~113)와 HWPX `hp:effects` 모델 사이의 변환.
//!
//! 꼬리는 [효과 플래그 u32][그림자][네온][부드러운 가장자리][반사] 순서이고 플래그 비트
//! 1·2·4·8이 각 효과의 유무다. 값 배치는 한컴 12.30 Mac 저장본(그림 도구 상자 견본 71개를
//! 같은 그림에 걸어 HWP·HWPX로 저장)과 바이트로 대조했다.

use super::image::{
    ColorEffect, EffectColor, EffectPoint, EffectRange, EffectRgb, PictureEffects, PictureGlow,
    PictureReflection, PictureShadow,
};
use serde_json::{json, Map, Value};

const SHADOW: u32 = 0x1;
const GLOW: u32 = 0x2;
const SOFT_EDGE: u32 = 0x4;
const REFLECTION: u32 = 0x8;

/// HWP 정렬 번호 ↔ HWPX `alignStyle`.
const ALIGN_STYLES: [&str; 9] = [
    "TOP_LEFT",
    "TOP",
    "TOP_RIGHT",
    "LEFT",
    "CENTER",
    "RIGHT",
    "BOTTOM_LEFT",
    "BOTTOM",
    "BOTTOM_RIGHT",
];

/// HWP 색 효과 종류 번호 ↔ HWPX `hp:effect@type`. 한컴 저장본에서 본 것만 둔다.
const COLOR_EFFECT_KINDS: [(u32, &str); 1] = [(0x204, "SAT_MOD")];

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn u32(&mut self) -> Option<u32> {
        let b = self.bytes.get(self.at..self.at + 4)?;
        self.at += 4;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f32(&mut self) -> Option<String> {
        let v = f32::from_bits(self.u32()?);
        v.is_finite().then(|| v.to_string())
    }

    fn point(&mut self) -> Option<EffectPoint> {
        Some(EffectPoint {
            x: Some(self.f32()?),
            y: Some(self.f32()?),
        })
    }

    fn align(&mut self) -> Option<String> {
        ALIGN_STYLES
            .get(self.u32()? as usize)
            .map(|name| name.to_string())
    }

    /// 색 정보(표 113). RGB(종류 0)만 길이를 안다.
    fn color(&mut self) -> Option<EffectColor> {
        if self.u32()? != 0 {
            return None;
        }
        let rgb = self.u32()?;
        let count = self.u32()?;
        let mut effects = Vec::new();
        for _ in 0..count.min(16) {
            let code = self.u32()?;
            let kind = COLOR_EFFECT_KINDS.iter().find(|(c, _)| *c == code)?.1;
            effects.push(ColorEffect {
                kind: kind.to_string(),
                value: self.f32()?,
            });
        }
        if count > 16 {
            return None;
        }
        let channel = |shift: u32| Some(((rgb >> shift) & 0xff).to_string());
        Some(EffectColor {
            color_type: Some("RGB".to_string()),
            scheme_idx: Some("-1".to_string()),
            system_idx: Some("-1".to_string()),
            preset_idx: Some("-1".to_string()),
            rgb: Some(EffectRgb {
                r: channel(16),
                g: channel(8),
                b: channel(0),
            }),
            effects,
        })
    }
}

/// 꼬리(`raw_picture_extra`)의 효과 정보를 읽는다. 반환은 효과와 그 뒤(원본 크기)의 위치다.
/// 모르는 플래그·색 종류나 잘린 꼬리는 None이다(원문은 그대로 둔다).
pub(crate) fn decode_hwp_picture_effects(extra: &[u8]) -> Option<(PictureEffects, usize)> {
    let mut r = Reader {
        bytes: extra,
        at: 5,
    };
    let flags = r.u32()?;
    if flags & !0xf != 0 {
        return None;
    }
    let mut effects = PictureEffects::default();
    if flags & SHADOW != 0 {
        let style = match r.u32()? {
            0 => "OUTSIDE",
            1 => "INSIDE",
            _ => return None,
        };
        effects.shadow = Some(PictureShadow {
            style: Some(style.to_string()),
            alpha: Some(r.f32()?),
            radius: Some(r.f32()?),
            direction: Some(r.f32()?),
            distance: Some(r.f32()?),
            align_style: Some(r.align()?),
            skew: Some(r.point()?),
            scale: Some(r.point()?),
            rotation_style: Some(r.u32()?.to_string()),
            color: Some(r.color()?),
        });
    }
    if flags & GLOW != 0 {
        effects.glow = Some(PictureGlow {
            alpha: Some(r.f32()?),
            radius: Some(r.f32()?),
            color: Some(r.color()?),
        });
    }
    if flags & SOFT_EDGE != 0 {
        effects.soft_edge_radius = Some(r.f32()?.parse().ok()?);
    }
    if flags & REFLECTION != 0 {
        let align_style = Some(r.align()?);
        let radius = Some(r.f32()?);
        let direction = Some(r.f32()?);
        let distance = Some(r.f32()?);
        let skew = Some(r.point()?);
        let scale = Some(r.point()?);
        let rotation_style = Some(r.u32()?.to_string());
        let (alpha_start, pos_start) = (r.f32()?, r.f32()?);
        let (alpha_end, pos_end) = (r.f32()?, r.f32()?);
        effects.reflection = Some(PictureReflection {
            align_style,
            radius,
            direction,
            distance,
            rotation_style,
            fade_direction: Some(r.f32()?),
            skew,
            scale,
            alpha: Some(EffectRange {
                start: Some(alpha_start),
                end: Some(alpha_end),
            }),
            pos: Some(EffectRange {
                start: Some(pos_start),
                end: Some(pos_end),
            }),
        });
    }
    Some((effects, r.at))
}

fn num(v: Option<&str>) -> f32 {
    v.and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
}

fn put_f32(out: &mut Vec<u8>, v: Option<&str>) {
    out.extend_from_slice(&num(v).to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_point(out: &mut Vec<u8>, p: Option<&EffectPoint>) {
    put_f32(out, p.and_then(|p| p.x.as_deref()));
    put_f32(out, p.and_then(|p| p.y.as_deref()));
}

fn put_align(out: &mut Vec<u8>, v: Option<&str>) {
    let index = ALIGN_STYLES.iter().position(|name| Some(*name) == v);
    put_u32(out, index.unwrap_or(0) as u32);
}

fn put_color(out: &mut Vec<u8>, color: Option<&EffectColor>) {
    let channel = |v: Option<&String>| num(v.map(String::as_str)).clamp(0.0, 255.0) as u32;
    let rgb = color.and_then(|c| c.rgb.as_ref());
    let value = (channel(rgb.and_then(|c| c.r.as_ref())) << 16)
        | (channel(rgb.and_then(|c| c.g.as_ref())) << 8)
        | channel(rgb.and_then(|c| c.b.as_ref()));
    put_u32(out, 0);
    put_u32(out, value);
    let known: Vec<_> = color
        .map(|c| c.effects.as_slice())
        .unwrap_or_default()
        .iter()
        .filter_map(|e| {
            let code = COLOR_EFFECT_KINDS.iter().find(|(_, k)| *k == e.kind)?.0;
            Some((code, e.value.as_str()))
        })
        .collect();
    put_u32(out, known.len() as u32);
    for (code, value) in known {
        put_u32(out, code);
        put_f32(out, Some(value));
    }
}

/// 효과 모델을 HWP 꼬리의 효과 정보(플래그부터)로 쓴다. 효과가 없으면 플래그 0 네 바이트다.
pub(crate) fn encode_hwp_picture_effects(effects: &PictureEffects) -> Vec<u8> {
    let soft = effects
        .soft_edge_radius
        .filter(|r| r.is_finite() && *r >= 0.0);
    let flags = (effects.shadow.is_some() as u32 * SHADOW)
        | (effects.glow.is_some() as u32 * GLOW)
        | (soft.is_some() as u32 * SOFT_EDGE)
        | (effects.reflection.is_some() as u32 * REFLECTION);
    let mut out = Vec::new();
    put_u32(&mut out, flags);
    if let Some(s) = &effects.shadow {
        put_u32(&mut out, (s.style.as_deref() == Some("INSIDE")) as u32);
        put_f32(&mut out, s.alpha.as_deref());
        put_f32(&mut out, s.radius.as_deref());
        put_f32(&mut out, s.direction.as_deref());
        put_f32(&mut out, s.distance.as_deref());
        put_align(&mut out, s.align_style.as_deref());
        put_point(&mut out, s.skew.as_ref());
        put_point(&mut out, s.scale.as_ref());
        put_u32(&mut out, num(s.rotation_style.as_deref()) as u32);
        put_color(&mut out, s.color.as_ref());
    }
    if let Some(g) = &effects.glow {
        put_f32(&mut out, g.alpha.as_deref());
        put_f32(&mut out, g.radius.as_deref());
        put_color(&mut out, g.color.as_ref());
    }
    if let Some(radius) = soft {
        out.extend_from_slice(&(radius as f32).to_le_bytes());
    }
    if let Some(r) = &effects.reflection {
        put_align(&mut out, r.align_style.as_deref());
        put_f32(&mut out, r.radius.as_deref());
        put_f32(&mut out, r.direction.as_deref());
        put_f32(&mut out, r.distance.as_deref());
        put_point(&mut out, r.skew.as_ref());
        put_point(&mut out, r.scale.as_ref());
        put_u32(&mut out, num(r.rotation_style.as_deref()) as u32);
        put_f32(&mut out, r.alpha.as_ref().and_then(|a| a.start.as_deref()));
        put_f32(&mut out, r.pos.as_ref().and_then(|a| a.start.as_deref()));
        put_f32(&mut out, r.alpha.as_ref().and_then(|a| a.end.as_deref()));
        put_f32(&mut out, r.pos.as_ref().and_then(|a| a.end.as_deref()));
        put_f32(&mut out, r.fade_direction.as_deref());
    }
    out
}

// ─── 앱 API JSON (`getPictureEffects`·`setPictureEffects`) ───

fn json_num(v: Option<&String>) -> Value {
    v.and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .map_or(Value::Null, |v| json!(v))
}

fn color_json(color: Option<&EffectColor>, out: &mut Map<String, Value>) {
    let Some(color) = color else { return };
    let channel = |v: Option<&String>| num(v.map(String::as_str)).clamp(0.0, 255.0) as u8;
    if let Some(rgb) = &color.rgb {
        out.insert(
            "color".into(),
            json!(format!(
                "#{:02x}{:02x}{:02x}",
                channel(rgb.r.as_ref()),
                channel(rgb.g.as_ref()),
                channel(rgb.b.as_ref())
            )),
        );
    }
    if !color.effects.is_empty() {
        let effects: Vec<Value> = color
            .effects
            .iter()
            .map(|e| json!({"type": e.kind, "value": json_num(Some(&e.value))}))
            .collect();
        out.insert("colorEffects".into(), Value::Array(effects));
    }
}

fn point_json(p: Option<&EffectPoint>, x: &str, y: &str, out: &mut Map<String, Value>) {
    out.insert(x.into(), json_num(p.and_then(|p| p.x.as_ref())));
    out.insert(y.into(), json_num(p.and_then(|p| p.y.as_ref())));
}

/// 그림 효과를 앱 JSON으로 쓴다. 없는 효과는 null이다.
pub(crate) fn picture_effects_to_json(effects: &PictureEffects) -> Value {
    let shadow = effects.shadow.as_ref().map(|s| {
        let mut o = Map::new();
        o.insert("style".into(), json!(s.style));
        o.insert("alpha".into(), json_num(s.alpha.as_ref()));
        o.insert("radius".into(), json_num(s.radius.as_ref()));
        o.insert("direction".into(), json_num(s.direction.as_ref()));
        o.insert("distance".into(), json_num(s.distance.as_ref()));
        o.insert("alignStyle".into(), json!(s.align_style));
        o.insert("rotationStyle".into(), json_num(s.rotation_style.as_ref()));
        point_json(s.skew.as_ref(), "skewX", "skewY", &mut o);
        point_json(s.scale.as_ref(), "scaleX", "scaleY", &mut o);
        color_json(s.color.as_ref(), &mut o);
        Value::Object(o)
    });
    let glow = effects.glow.as_ref().map(|g| {
        let mut o = Map::new();
        o.insert("alpha".into(), json_num(g.alpha.as_ref()));
        o.insert("radius".into(), json_num(g.radius.as_ref()));
        color_json(g.color.as_ref(), &mut o);
        Value::Object(o)
    });
    let reflection = effects.reflection.as_ref().map(|r| {
        let mut o = Map::new();
        o.insert("alignStyle".into(), json!(r.align_style));
        o.insert("radius".into(), json_num(r.radius.as_ref()));
        o.insert("direction".into(), json_num(r.direction.as_ref()));
        o.insert("distance".into(), json_num(r.distance.as_ref()));
        o.insert("rotationStyle".into(), json_num(r.rotation_style.as_ref()));
        o.insert("fadeDirection".into(), json_num(r.fade_direction.as_ref()));
        point_json(r.skew.as_ref(), "skewX", "skewY", &mut o);
        point_json(r.scale.as_ref(), "scaleX", "scaleY", &mut o);
        let (alpha, pos) = (r.alpha.as_ref(), r.pos.as_ref());
        o.insert(
            "startAlpha".into(),
            json_num(alpha.and_then(|a| a.start.as_ref())),
        );
        o.insert(
            "endAlpha".into(),
            json_num(alpha.and_then(|a| a.end.as_ref())),
        );
        o.insert(
            "startPos".into(),
            json_num(pos.and_then(|a| a.start.as_ref())),
        );
        o.insert("endPos".into(), json_num(pos.and_then(|a| a.end.as_ref())));
        Value::Object(o)
    });
    json!({
        "shadow": shadow,
        "glow": glow,
        "softEdge": effects.soft_edge_radius,
        "reflection": reflection,
    })
}

/// 숫자를 한컴 저장본 꼴(f32 최단 표기: 0.5·600·0.997)로 쓴다.
fn text(o: &Map<String, Value>, key: &str) -> Option<String> {
    let v = o.get(key)?;
    match v {
        Value::String(s) => Some(s.clone()),
        _ => v
            .as_f64()
            .filter(|v| v.is_finite())
            .map(|v| (v as f32).to_string()),
    }
}

fn point(o: &Map<String, Value>, x: &str, y: &str) -> Option<EffectPoint> {
    Some(EffectPoint {
        x: Some(text(o, x).unwrap_or_else(|| "0".into())),
        y: Some(text(o, y).unwrap_or_else(|| "0".into())),
    })
}

fn color(o: &Map<String, Value>) -> Result<Option<EffectColor>, String> {
    let Some(hex) = o.get("color").and_then(Value::as_str) else {
        return Ok(None);
    };
    let hex = hex.trim_start_matches('#');
    let v = u32::from_str_radix(hex, 16)
        .ok()
        .filter(|_| hex.len() == 6)
        .ok_or_else(|| format!("색 값이 #RRGGBB 꼴이 아닙니다: {hex}"))?;
    let effects = o
        .get("colorEffects")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|e| {
                    let e = e.as_object()?;
                    Some(ColorEffect {
                        kind: e.get("type")?.as_str()?.to_string(),
                        value: text(e, "value")?,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Some(EffectColor {
        color_type: Some("RGB".into()),
        scheme_idx: Some("-1".into()),
        system_idx: Some("-1".into()),
        preset_idx: Some("-1".into()),
        rgb: Some(EffectRgb {
            r: Some(((v >> 16) & 0xff).to_string()),
            g: Some(((v >> 8) & 0xff).to_string()),
            b: Some((v & 0xff).to_string()),
        }),
        effects,
    }))
}

/// 앱 JSON을 지금 효과에 덮는다. 없는 키는 그대로, null은 그 효과를 지운다.
pub(crate) fn apply_picture_effects_json(
    current: &PictureEffects,
    json: &str,
) -> Result<PictureEffects, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("그림 효과 JSON: {e}"))?;
    let o = v.as_object().ok_or("그림 효과 JSON은 객체여야 합니다")?;
    let mut next = current.clone();
    let object =
        |key: &str| -> Option<Option<&Map<String, Value>>> { o.get(key).map(|v| v.as_object()) };
    if let Some(s) = object("shadow") {
        next.shadow = match s {
            None => None,
            Some(s) => Some(PictureShadow {
                style: text(s, "style").or(Some("OUTSIDE".into())),
                alpha: text(s, "alpha"),
                radius: text(s, "radius"),
                direction: text(s, "direction"),
                distance: text(s, "distance"),
                align_style: text(s, "alignStyle"),
                rotation_style: text(s, "rotationStyle").or(Some("0".into())),
                skew: point(s, "skewX", "skewY"),
                scale: point(s, "scaleX", "scaleY"),
                color: color(s)?,
            }),
        };
    }
    if let Some(g) = object("glow") {
        next.glow = match g {
            None => None,
            Some(g) => Some(PictureGlow {
                alpha: text(g, "alpha"),
                radius: text(g, "radius"),
                color: color(g)?,
            }),
        };
    }
    if o.contains_key("softEdge") {
        next.soft_edge_radius = o
            .get("softEdge")
            .and_then(Value::as_f64)
            .filter(|r| r.is_finite() && *r >= 0.0);
    }
    if let Some(r) = object("reflection") {
        next.reflection = r.map(|r| PictureReflection {
            align_style: text(r, "alignStyle"),
            radius: text(r, "radius"),
            direction: text(r, "direction"),
            distance: text(r, "distance"),
            rotation_style: text(r, "rotationStyle").or(Some("0".into())),
            fade_direction: text(r, "fadeDirection"),
            skew: point(r, "skewX", "skewY"),
            scale: point(r, "scaleX", "scaleY"),
            alpha: Some(EffectRange {
                start: text(r, "startAlpha"),
                end: text(r, "endAlpha"),
            }),
            pos: Some(EffectRange {
                start: text(r, "startPos"),
                end: text(r, "endPos"),
            }),
        });
    }
    Ok(next)
}
