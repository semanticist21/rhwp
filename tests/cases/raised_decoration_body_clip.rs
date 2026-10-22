//! 첫 줄의 위 밑줄·강조점은 글줄 상자 위에 그려진다. 본문 clip 이 그 잉크를
//! 포함해야 SVG/PDF 에서 잘리지 않으며, 글줄·캐럿 기하는 바뀌지 않아야 한다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::renderer::render_tree::{BoundingBox, RenderNode, RenderNodeType};
use rhwp::wasm_api::HwpDocument;

const TEXT: &str = "H 첫째 줄";

fn fixture(props: &str) -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, TEXT).unwrap();
    let n = TEXT.chars().count();
    doc.apply_char_format_native(0, 0, 0, n, r#"{"fontSize":3600}"#)
        .unwrap();
    if !props.is_empty() {
        doc.apply_char_format_native(0, 0, 0, n, props).unwrap();
    }
    doc
}

fn body_clip(doc: &HwpDocument) -> BoundingBox {
    fn find(node: &RenderNode) -> Option<BoundingBox> {
        if let RenderNodeType::Body {
            clip_rect: Some(clip),
        } = &node.node_type
        {
            return Some(*clip);
        }
        node.children.iter().find_map(find)
    }
    find(&doc.build_page_render_tree(0).unwrap().root).expect("본문 clip")
}

fn attr(tag: &str, name: &str) -> f64 {
    let rest = tag.split_once(&format!(" {name}=\"")).unwrap().1;
    rest.split_once('"').unwrap().0.parse().unwrap()
}

fn tags(svg: &str, name: &str) -> Vec<String> {
    svg.split(&format!("<{name} "))
        .skip(1)
        .map(|tag| format!(" {tag}"))
        .collect()
}

fn probe(name: &str) -> HwpDocument {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("mydocs/pr/assets/raised-decoration-hancom")
        .join(format!("{name}.hwp"));
    let mut doc = HwpDocument::from_bytes(&std::fs::read(path).unwrap()).unwrap();
    doc.set_hangul2024_compat(true);
    assert_eq!(doc.page_count(), 1, "독립 Print PDF와 쪽수 일치");
    doc
}

fn near_print(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 2.0,
        "96dpi Print 좌표: {actual} / {expected}"
    );
}

/// SVG 가 발행한 `<line stroke="#d03050">` 들의 잉크 상단(중심 - 획 반).
fn top_line_ink(svg: &str) -> f64 {
    svg.split("<line ")
        .skip(1)
        .map(|c| format!(" {}", c.split_once('>').unwrap().0))
        .filter(|tag| tag.contains("stroke=\"#d03050\""))
        .map(|tag| attr(&tag, "y1") - attr(&tag, "stroke-width") / 2.0)
        .fold(f64::INFINITY, f64::min)
}

/// 독립 Print가 쓰는 HCR Batang U+0307 잉크 상단이다.
fn top_dot_ink(svg: &str) -> f64 {
    svg.split("<text ")
        .skip(1)
        .map(|c| format!(" {c}"))
        .filter(|c| c.contains(">\u{0307}</text>"))
        .map(|tag| attr(&tag, "y") - attr(&tag, "font-size") * 0.675)
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn first_line_top_underline_ink_is_inside_body_clip() {
    let plain = body_clip(&fixture(""));
    for shape in [0, 6, 10] {
        let doc = fixture(&format!(
            r##"{{"underlineType":"Top","underlineShape":{shape},"underlineColor":"#d03050"}}"##
        ));
        let ink_top = top_line_ink(&doc.render_page_svg_native(0).unwrap());
        assert!(ink_top < plain.y, "fixture: 위 밑줄이 본문 위로 나가야 함");
        let clip = body_clip(&doc);
        assert!(
            clip.y <= ink_top + 1e-6,
            "shape {shape}: {clip:?} / {ink_top}"
        );
        assert!(
            (clip.y + clip.height - plain.y - plain.height).abs() < 1e-6,
            "본문 하단은 그대로"
        );
    }
}

#[test]
fn first_line_emphasis_dot_ink_is_inside_body_clip() {
    let plain_doc = fixture("");
    let plain = body_clip(&plain_doc);
    let doc = fixture(r#"{"emphasisDot":1}"#);
    let ink_top = top_dot_ink(&doc.render_page_svg_native(0).unwrap());
    assert!(ink_top < plain.y, "fixture: 강조점이 본문 위로 나가야 함");
    assert!(body_clip(&doc).y <= ink_top + 1e-6);
    // 글자 상자를 키우지 않으므로 캐럿 위치는 그대로다.
    assert_eq!(
        doc.get_cursor_rect_native(0, 0, 1).unwrap(),
        plain_doc.get_cursor_rect_native(0, 0, 1).unwrap()
    );
}

#[test]
fn hancom_solid_top_underline_keeps_print_height_and_width() {
    let doc = probe("top-probe-shape-0");
    let svg = doc.render_page_svg_native(0).unwrap();
    let mut baselines: Vec<(f64, f64)> = tags(&svg, "text")
        .iter()
        .map(|tag| (attr(tag, "y"), attr(tag, "font-size")))
        .collect();
    baselines.sort_by(|a, b| a.0.total_cmp(&b.0));
    baselines.dedup();
    assert_eq!(baselines.len(), 16, "Top/Bottom 원문 문단 보존");
    let lines = tags(&svg, "line");
    let clip = body_clip(&doc);
    // 독립 Hancom Print PDF의 9/10/12/15/18/24/36/48pt 실선 실측(em)이다.
    for (pair, (offset, width)) in baselines.chunks_exact(2).zip([
        (-0.86664, 0.04004),
        (-0.87957, 0.03618),
        (-0.87004, 0.04004),
        (-0.87198, 0.04004),
        (-0.86668, 0.04004),
        (-0.87002, 0.04000),
        (-0.87000, 0.04002),
        (-0.87001, 0.04002),
    ]) {
        let (baseline, size) = pair[0];
        let top: Vec<_> = lines
            .iter()
            .filter(|tag| tag.contains("stroke=\"#cc0000\""))
            .filter(|tag| (attr(tag, "y1") - baseline).abs() < size * 1.3)
            .collect();
        assert!(!top.is_empty(), "위 실선 누락 방지");
        for line in top {
            let y = attr(line, "y1");
            let stroke = attr(line, "stroke-width");
            assert!(((y - baseline) / size - offset).abs() < 0.02);
            assert!((stroke / size - width).abs() < 0.005);
            assert!(clip.y <= y - stroke / 2.0, "첫 줄 잉크를 본문 clip에 포함");
        }
    }
}

#[test]
fn hancom_dashed_top_underline_keeps_print_geometry_across_font_runs() {
    let doc = probe("top-probe-shape-1");
    let svg = doc.render_page_svg_native(0).unwrap();
    let mut baselines: Vec<_> = tags(&svg, "text")
        .iter()
        .map(|tag| (attr(tag, "y"), attr(tag, "font-size")))
        .collect();
    baselines.sort_by(|a, b| a.0.total_cmp(&b.0));
    baselines.dedup();
    assert_eq!(baselines.len(), 16);
    let lines = tags(&svg, "line");
    // 독립 Print PDF의 높이·굵기(em), 첫 획 길이·주기·마지막 끝점(96dpi px)이다.
    let measured = [
        (-0.86664, 0.04004, 3.35867, 5.27733, 153.67733, 8),
        (-0.87957, 0.03618, 3.83867, 6.07733, 165.83067, 9),
        (-0.87004, 0.04004, 4.47733, 7.35600, 176.70533, 9),
        (-0.87198, 0.04004, 5.75733, 9.27600, 192.85600, 9),
        (-0.86668, 0.04004, 6.87600, 11.03467, 208.52800, 9),
        (-0.87002, 0.04000, 9.11600, 14.55200, 238.91200, 9),
        (-0.87000, 0.04002, 13.59333, 21.90800, 302.23733, 9),
        (-0.87001, 0.04002, 18.23067, 29.10533, 364.44400, 9),
    ];
    let clip = body_clip(&doc);
    for (pair, (offset, width, on, period, end, count)) in baselines.chunks_exact(2).zip(measured) {
        let (baseline, size) = pair[0];
        let mut strokes: Vec<_> = lines
            .iter()
            .filter(|tag| tag.contains("stroke=\"#cc0000\""))
            .filter(|tag| (attr(tag, "y1") - baseline).abs() < size * 1.3)
            .collect();
        assert!(!strokes.is_empty());
        strokes.sort_by(|a, b| attr(a, "x1").total_cmp(&attr(b, "x1")));
        let mut spans: Vec<(f64, f64)> = Vec::new();
        for line in strokes {
            let y = attr(line, "y1");
            let stroke = attr(line, "stroke-width");
            assert!(((y - baseline) / size - offset).abs() < 0.02);
            assert!((stroke / size - width).abs() < 0.005);
            assert!(clip.y <= y - stroke / 2.0);
            let (left, right) = (attr(line, "x1"), attr(line, "x2"));
            // run 경계에서 나뉜 같은 획을 합쳐 Print의 실제 획과 대조한다.
            if let Some(last) = spans
                .last_mut()
                .filter(|last| (last.1 - left).abs() < 0.001)
            {
                last.1 = right;
            } else {
                spans.push((left, right));
            }
        }
        assert_eq!(
            spans.len(),
            count,
            "언어별 run에서 파선이 다시 시작하지 않는다"
        );
        for (i, (left, right)) in spans.into_iter().enumerate() {
            let print_left = 113.37867 + i as f64 * period;
            near_print(left, print_left);
            near_print(right, (print_left + on).min(end));
        }
    }
}

#[test]
fn dashed_top_underline_phase_stays_with_its_contiguous_line() {
    use rhwp::paint::{LayerBuilder, RenderProfile};
    use rhwp::renderer::render_tree::PageRenderTree;

    fn find(node: &RenderNode) -> Option<RenderNode> {
        if matches!(node.node_type, RenderNodeType::TextLine(_))
            && node.children.iter().any(|child| {
                matches!(&child.node_type, RenderNodeType::TextRun(run)
                    if run.style.underline == rhwp::model::style::UnderlineType::Top)
            })
        {
            return Some(node.clone());
        }
        node.children.iter().find_map(find)
    }
    fn offsets(value: &serde_json::Value, output: &mut Vec<f64>) {
        match value {
            serde_json::Value::Object(map) => {
                if map.get("type").and_then(|v| v.as_str()) == Some("textDecoration") {
                    output.push(
                        map.get("underlineDashOffset")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0),
                    );
                }
                for child in map.values() {
                    offsets(child, output);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    offsets(item, output);
                }
            }
            _ => {}
        }
    }
    let original = find(
        &probe("top-probe-shape-1")
            .build_page_render_tree(0)
            .unwrap()
            .root,
    )
    .unwrap();
    assert_eq!(
        original.children.len(),
        2,
        "영문·한글 run 경계가 있는 실제 원본"
    );
    let width = original.children[0].bbox.width;
    for change in ["none", "gap", "color", "new-line"] {
        let mut line = original.clone();
        match change {
            "gap" => line.children[1].bbox.x += 1.0,
            "color" => {
                let RenderNodeType::TextRun(run) = &mut line.children[1].node_type else {
                    unreachable!()
                };
                run.style.underline_color ^= 1;
            }
            _ => {}
        }
        let mut tree = PageRenderTree::new(0, 794.0, 1122.0);
        tree.root.children.push(line);
        if change == "new-line" {
            let mut next = original.clone();
            next.id += 100_000;
            for child in &mut next.children {
                child.id += 100_000;
            }
            tree.root.children.push(next);
        }
        let layer = LayerBuilder::new(RenderProfile::Print).build(&tree);
        let mut phases = Vec::new();
        offsets(
            &serde_json::from_str(&layer.to_json()).unwrap(),
            &mut phases,
        );
        let expected = match change {
            "gap" | "color" => vec![0.0, 0.0],
            "new-line" => vec![0.0, width, 0.0, width],
            _ => vec![0.0, width],
        };
        assert_eq!(phases, expected, "{change}");
    }
}

#[test]
fn hancom_multi_underline_keeps_bottom_strokes_above_baseline() {
    // 같은 이름의 pdf/raised-decoration-hancom Print PDF, 9~48pt 32쌍이다.
    for (shape, lift, print_y) in [
        (7, 1.11, 131.056),
        (8, 1.20, 130.0973),
        (9, 1.20, 130.5773),
        (10, 1.20, 130.0973),
    ] {
        let svg = probe(&format!("top-probe-shape-{shape}"))
            .render_page_svg_native(0)
            .unwrap();
        let mut baselines: Vec<(f64, f64)> = tags(&svg, "text")
            .iter()
            .map(|tag| (attr(tag, "y"), attr(tag, "font-size")))
            .collect();
        baselines.sort_by(|a, b| a.0.total_cmp(&b.0));
        baselines.dedup();
        assert_eq!(baselines.len(), 16, "원문 문단 누락·중복 방지");
        let lines = tags(&svg, "line");
        let first = lines
            .iter()
            .find(|tag| tag.contains("stroke=\"#cc0000\""))
            .unwrap();
        near_print(attr(first, "x1"), 113.3787);
        near_print(attr(first, "y1"), print_y);
        for pair in baselines.chunks_exact(2) {
            let strokes = |(baseline, size): (f64, f64), color: &str| {
                let mut rows: Vec<(f64, f64)> = lines
                    .iter()
                    .filter(|tag| tag.contains(&format!("stroke=\"{color}\"")))
                    .filter(|tag| (attr(tag, "y1") - baseline).abs() < size * 1.3)
                    .map(|tag| {
                        (
                            (attr(tag, "y1") - baseline) / size,
                            attr(tag, "stroke-width") / size,
                        )
                    })
                    .collect();
                rows.sort_by(|a, b| a.0.total_cmp(&b.0));
                rows.dedup();
                rows
            };
            let top = strokes(pair[0], "#cc0000");
            let bottom = strokes(pair[1], "#0000cc");
            assert_eq!(top.len(), if shape == 10 { 3 } else { 2 });
            assert_eq!(top.len(), bottom.len());
            for (top, bottom) in top.iter().zip(&bottom) {
                assert!((top.0 - bottom.0 + lift).abs() < 0.02, "shape {shape}");
                assert!((top.1 - bottom.1).abs() < 0.001, "획 굵기 보존");
            }
        }
    }
}

#[test]
fn hancom_emphasis_marks_skip_spaces_and_keep_print_glyphs() {
    for (kind, glyph) in [
        (1, "\u{0307}"),
        (2, "\u{030a}"),
        (3, "\u{030c}"),
        (4, "\u{0303}"),
    ] {
        let svg = probe(&format!("emphasis-probe-{kind}"))
            .render_page_svg_native(0)
            .unwrap();
        let texts = tags(&svg, "text");
        let marks: Vec<_> = texts
            .iter()
            .filter(|tag| tag.contains(&format!(">{glyph}</text>")))
            .collect();
        // 3개 글꼴 × 4개 크기 × 문장 부호 포함 7자, 각 문단의 공백 2개는 제외한다.
        assert_eq!(marks.len(), 84, "kind {kind}");
        for size in [12.0, 16.0, 32.0, 48.0] {
            assert_eq!(
                marks
                    .iter()
                    .filter(|tag| (attr(tag, "font-size") - size).abs() < 0.01)
                    .count(),
                21
            );
        }
        for mark in &marks {
            assert!(mark.contains("font-family=\"HCR Batang,함초롬바탕,serif\""));
            let size = attr(mark, "font-size");
            let baseline = attr(mark, "y") + size * 0.37;
            assert!(texts
                .iter()
                .any(|tag| !tag.contains(&format!(">{glyph}</text>"))
                    && (attr(tag, "font-size") - size).abs() < 0.01
                    && (attr(tag, "y") - baseline).abs() < 0.01));
        }
        // 각 Print PDF 첫 U+030x의 텍스트 행렬을 72dpi에서 96dpi로 환산했다.
        near_print(attr(marks[0], "x"), 120.5747);
        near_print(attr(marks[0], "y"), 137.9293);
    }
    for (kind, count, print_y) in [(5, 84, 138.4607), (6, 168, 137.328)] {
        let svg = probe(&format!("emphasis-probe-{kind}"))
            .render_page_svg_native(0)
            .unwrap();
        let marks = tags(&svg, "ellipse");
        assert_eq!(marks.len(), count, "kind {kind}");
        near_print(attr(&marks[0], "cx"), 112.69);
        near_print(attr(&marks[0], "cy"), print_y);
        if kind == 6 {
            for pair in marks.chunks_exact(2) {
                assert_eq!(attr(&pair[0], "cx"), attr(&pair[1], "cx"));
                assert!(attr(&pair[0], "cy") < attr(&pair[1], "cy"));
            }
        }
    }
}

#[test]
fn hancom_body_and_cell_decorations_keep_print_positions() {
    for (name, x, y) in [
        ("body-top-36pt", 113.3787, 127.2213),
        ("cell-top-10-36pt", 123.9333, 136.6507),
    ] {
        let svg = probe(name).render_page_svg_native(0).unwrap();
        let line = tags(&svg, "line")
            .into_iter()
            .find(|tag| tag.contains("stroke=\"#cc0000\""))
            .unwrap();
        near_print(attr(&line, "x1"), x);
        near_print(attr(&line, "y1"), y);
    }
    let svg = probe("body-emphasis-36pt")
        .render_page_svg_native(0)
        .unwrap();
    let marks: Vec<_> = tags(&svg, "text")
        .into_iter()
        .filter(|tag| tag.contains(">\u{0307}</text>"))
        .collect();
    assert_eq!(marks.len(), 7);
    near_print(attr(&marks[0], "x"), 147.2802);
    near_print(attr(&marks[0], "y"), 155.1904);
}
