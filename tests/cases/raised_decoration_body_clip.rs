//! 첫 줄의 위 밑줄·강조점은 글줄 상자 위에 그려진다. 본문 clip 이 그 잉크를
//! 포함해야 SVG/PDF 에서 잘리지 않으며, 글줄·캐럿 기하는 바뀌지 않아야 한다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::style::UnderlineType;
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

/// SVG가 발행한 밑줄 선·원의 실제 잉크 상단이다.
fn top_underline_ink(svg: &str) -> f64 {
    svg.split("<line ")
        .skip(1)
        .map(|c| format!(" {}", c.split_once('>').unwrap().0))
        .filter(|tag| tag.contains("stroke=\"#d03050\""))
        .map(|tag| attr(&tag, "y1") - attr(&tag, "stroke-width") / 2.0)
        .chain(
            tags(svg, "circle")
                .iter()
                .filter(|tag| tag.contains("fill=\"#d03050\""))
                .map(|tag| attr(tag, "cy") - attr(tag, "r")),
        )
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
        let ink_top = top_underline_ink(&doc.render_page_svg_native(0).unwrap());
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

fn assert_pattern_probe(
    name: &str,
    position: UnderlineType,
    measured: [(f64, f64, f64, f64, f64, usize); 8],
    dot_pattern: Option<(usize, [(f64, f64); 8])>,
) {
    let doc = probe(name);
    let svg = doc.render_page_svg_native(0).unwrap();
    let mut baselines: Vec<_> = tags(&svg, "text")
        .iter()
        .map(|tag| (attr(tag, "y"), attr(tag, "font-size")))
        .collect();
    baselines.sort_by(|a, b| a.0.total_cmp(&b.0));
    baselines.dedup();
    assert_eq!(baselines.len(), 16);
    let lines = tags(&svg, "line");
    let clip = body_clip(&doc);
    for (index, (pair, (offset, width, on, period, end, count))) in
        baselines.chunks_exact(2).zip(measured).enumerate()
    {
        let (baseline, size) = pair[usize::from(position == UnderlineType::Bottom)];
        let color = if position == UnderlineType::Top {
            "#cc0000"
        } else {
            "#0000cc"
        };
        let mut strokes: Vec<_> = lines
            .iter()
            .filter(|tag| tag.contains(&format!(r#"stroke="{color}""#)))
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
            "언어별 run에서 무늬가 다시 시작하지 않는다"
        );
        for (i, (left, right)) in spans.into_iter().enumerate() {
            let (cycle, extra, ink) = match dot_pattern {
                Some((dots, pattern)) if i % (dots + 1) != 0 => {
                    let (dot, gap) = pattern[index];
                    (
                        i / (dots + 1),
                        on + gap + (i % (dots + 1) - 1) as f64 * (dot + gap),
                        dot,
                    )
                }
                Some((dots, _)) => (i / (dots + 1), 0.0, on),
                None => (i, 0.0, on),
            };
            let print_left = 113.37867 + cycle as f64 * period + extra;
            near_print(left, print_left);
            near_print(right, (print_left + ink).min(end));
        }
    }
}

#[test]
fn hancom_dashed_top_underline_keeps_print_geometry_across_font_runs() {
    // 독립 Print PDF의 높이·굵기(em), 첫 획 길이·주기·마지막 끝점(96dpi px)이다.
    assert_pattern_probe(
        "top-probe-shape-1",
        UnderlineType::Top,
        [
            (-0.86664, 0.04004, 3.35867, 5.27733, 153.67733, 8),
            (-0.87957, 0.03618, 3.83867, 6.07733, 165.83067, 9),
            (-0.87004, 0.04004, 4.47733, 7.35600, 176.70533, 9),
            (-0.87198, 0.04004, 5.75733, 9.27600, 192.85600, 9),
            (-0.86668, 0.04004, 6.87600, 11.03467, 208.52800, 9),
            (-0.87002, 0.04000, 9.11600, 14.55200, 238.91200, 9),
            (-0.87000, 0.04002, 13.59333, 21.90800, 302.23733, 9),
            (-0.87001, 0.04002, 18.23067, 29.10533, 364.44400, 9),
        ],
        None,
    );
}

#[test]
fn hancom_dotted_top_underline_keeps_print_grid_across_font_runs() {
    // 같은 9~48pt 독립 Print PDF에서 점선 획을 실측했다.
    assert_pattern_probe(
        "top-probe-shape-2",
        UnderlineType::Top,
        [
            (-0.86664, 0.04004, 0.64000, 1.60000, 153.99733, 26),
            (-0.87957, 0.03618, 0.80000, 2.07867, 166.15067, 26),
            (-0.87004, 0.04004, 0.96000, 2.39867, 176.70533, 27),
            (-0.87198, 0.04004, 1.12000, 2.87867, 192.21600, 28),
            (-0.86668, 0.04004, 1.44000, 3.67867, 206.76933, 26),
            (-0.87002, 0.04000, 1.76000, 4.47733, 240.35067, 29),
            (-0.87000, 0.04002, 2.71867, 6.87600, 301.75733, 28),
            (-0.87001, 0.04002, 3.67867, 9.27600, 367.32267, 28),
        ],
        None,
    );
}

#[test]
fn hancom_dash_dot_top_underline_keeps_print_pattern_across_font_runs() {
    // 독립 Print PDF의 긴 획·반복 주기·끝점과 짧은 점·긴 획 다음 간격(96dpi px)이다.
    assert_pattern_probe(
        "top-probe-shape-3",
        UnderlineType::Top,
        [
            (-0.86664, 0.04004, 6.87600, 11.35467, 154.31733, 7),
            (-0.87957, 0.03618, 7.67600, 12.95333, 166.47067, 9),
            (-0.87004, 0.04004, 9.11600, 15.83200, 177.02533, 9),
            (-0.87198, 0.04004, 11.51467, 19.66933, 192.85600, 9),
            (-0.86668, 0.04004, 13.75333, 23.50800, 208.84800, 9),
            (-0.87002, 0.04000, 18.39067, 31.02400, 240.35067, 9),
            (-0.87000, 0.04002, 27.34533, 46.69467, 303.99600, 9),
            (-0.87001, 0.04002, 36.62000, 62.04667, 367.32267, 9),
        ],
        Some((
            1,
            [
                (0.63867, 1.92000),
                (0.80000, 2.23867),
                (0.96000, 2.87733),
                (1.12000, 3.51733),
                (1.43867, 4.15733),
                (1.75867, 5.43733),
                (2.71867, 8.31600),
                (3.67867, 10.87467),
            ],
        )),
    );
}

#[test]
fn hancom_dash_dot_dot_top_underline_keeps_print_pattern_across_font_runs() {
    // 독립 Print PDF의 긴 획·주기·끝점과 두 점의 길이·간격(96dpi px)이다.
    assert_pattern_probe(
        "top-probe-shape-4",
        UnderlineType::Top,
        [
            (-0.86664, 0.04004, 6.87600, 13.91333, 153.19733, 9),
            (-0.87957, 0.03618, 7.67600, 15.99200, 166.47067, 10),
            (-0.87004, 0.04004, 9.11600, 19.66933, 177.02533, 10),
            (-0.87198, 0.04004, 11.51467, 24.30667, 192.85600, 10),
            (-0.86668, 0.04004, 13.75333, 29.10533, 208.84800, 10),
            (-0.87002, 0.04000, 18.39067, 38.22000, 240.35067, 10),
            (-0.87000, 0.04002, 27.34533, 57.72933, 303.99600, 10),
            (-0.87001, 0.04002, 36.62000, 76.59867, 367.32267, 10),
        ],
        Some((
            2,
            [
                (0.63867, 1.92000),
                (0.80000, 2.23867),
                (0.96000, 2.87733),
                (1.12000, 3.51733),
                (1.43867, 4.15733),
                (1.75867, 5.43733),
                (2.71867, 8.31600),
                (3.67867, 10.87467),
            ],
        )),
    );
}

#[test]
fn hancom_long_dashed_top_underline_keeps_print_geometry_across_font_runs() {
    // 독립 Print PDF의 높이·굵기(em), 긴 획·주기·마지막 끝점(96dpi px)이다.
    assert_pattern_probe(
        "top-probe-shape-5",
        UnderlineType::Top,
        [
            (-0.86664, 0.04004, 6.87600, 8.79600, 154.47733, 5),
            (-0.87957, 0.03618, 7.67600, 9.91467, 166.47067, 6),
            (-0.87004, 0.04004, 9.11600, 11.99333, 177.02533, 6),
            (-0.87198, 0.04004, 11.51467, 15.03200, 192.85600, 6),
            (-0.86668, 0.04004, 13.75333, 17.91067, 208.84800, 6),
            (-0.87002, 0.04000, 18.39067, 23.82800, 240.35067, 6),
            (-0.87000, 0.04002, 27.34533, 35.66133, 303.99600, 6),
            (-0.87001, 0.04002, 36.62000, 47.49467, 367.32267, 6),
        ],
        None,
    );
}

#[test]
fn hancom_round_dotted_top_underline_keeps_print_circles_across_font_runs() {
    let doc = probe("top-probe-shape-6");
    let svg = doc.render_page_svg_native(0).unwrap();
    let mut baselines: Vec<_> = tags(&svg, "text")
        .iter()
        .map(|tag| (attr(tag, "y"), attr(tag, "font-size")))
        .collect();
    baselines.sort_by(|a, b| a.0.total_cmp(&b.0));
    baselines.dedup();
    assert_eq!(baselines.len(), 16, "Top/Bottom 원문 문단 보존");
    let circles = tags(&svg, "circle");
    let clip = body_clip(&doc);
    // 독립 Print의 원 중심·외곽 반지름·첫 중심·주기·마지막 중심(96dpi px)이다.
    for (pair, (cy, radius, first, period, last, count)) in baselines.chunks_exact(2).zip([
        (131.93600, 0.32000, 113.29867, 1.60000, 153.27733, 26),
        (170.13400, 0.48000, 113.29867, 2.07933, 165.27067, 26),
        (212.80733, 0.48000, 113.29867, 2.39867, 175.66533, 27),
        (263.79133, 0.63933, 113.29933, 2.87800, 191.01733, 28),
        (327.72200, 0.79933, 113.29933, 3.67800, 205.25000, 26),
        (404.27800, 0.95933, 113.29933, 4.47733, 238.67133, 29),
        (506.24667, 1.43933, 113.29933, 6.87600, 298.95933, 28),
        (659.35933, 1.91933, 113.29933, 9.27467, 363.72467, 28),
    ]) {
        let (baseline, size) = pair[0];
        let mut dots: Vec<_> = circles
            .iter()
            .filter(|tag| tag.contains("fill=\"#cc0000\""))
            .filter(|tag| (attr(tag, "cy") - baseline).abs() < size * 1.3)
            .collect();
        dots.sort_by(|a, b| attr(a, "cx").total_cmp(&attr(b, "cx")));
        assert_eq!(
            dots.len(),
            count,
            "run 경계에서 원을 중복하거나 누락하지 않는다"
        );
        for (i, dot) in dots.iter().enumerate() {
            let r = attr(dot, "r");
            near_print(attr(dot, "cy"), cy);
            near_print(r, radius);
            assert!((r - radius).abs() / size < 0.002, "Print 원 크기 비율 보존");
            near_print(attr(dot, "cx"), first + i as f64 * period);
            assert!(clip.y <= attr(dot, "cy") - r);
            assert!(clip.x <= attr(dot, "cx") - r);
        }
        near_print(attr(dots.last().unwrap(), "cx"), last);
    }
    assert_round_run_boundaries(&doc, UnderlineType::Top);
}

#[test]
fn hancom_round_dotted_bottom_underline_keeps_print_circles_across_font_runs() {
    let doc = probe("top-probe-shape-6");
    let svg = doc.render_page_svg_native(0).unwrap();
    let mut baselines: Vec<_> = tags(&svg, "text")
        .iter()
        .map(|tag| (attr(tag, "y"), attr(tag, "font-size")))
        .collect();
    baselines.sort_by(|a, b| a.0.total_cmp(&b.0));
    baselines.dedup();
    assert_eq!(baselines.len(), 16, "Top/Bottom 원문 문단 보존");
    let circles = tags(&svg, "circle");
    // 같은 독립 Print PDF의 아래 원 중심·외곽 반지름·첫 중심·주기·마지막 중심이다.
    for (pair, (cy, radius, first, period, last, count)) in baselines.chunks_exact(2).zip([
        (163.42133, 0.32000, 113.29867, 1.60000, 153.27733, 26),
        (205.29533, 0.48000, 113.29867, 2.07933, 165.27067, 26),
        (255.00067, 0.48000, 113.29867, 2.39867, 175.66533, 27),
        (316.53400, 0.63933, 113.29933, 2.87800, 191.01733, 28),
        (391.01267, 0.79933, 113.29933, 3.67800, 205.25000, 26),
        (488.66533, 0.95933, 113.29933, 4.47733, 234.19400, 28),
        (632.82800, 1.43933, 113.29933, 6.87600, 298.95933, 28),
        (827.97400, 1.91933, 113.29933, 9.27467, 354.44933, 27),
    ]) {
        let (baseline, size) = pair[1];
        let mut dots: Vec<_> = circles
            .iter()
            .filter(|tag| tag.contains("fill=\"#0000cc\""))
            .filter(|tag| (attr(tag, "cy") - baseline).abs() < size * 1.3)
            .collect();
        dots.sort_by(|a, b| attr(a, "cx").total_cmp(&attr(b, "cx")));
        assert_eq!(
            dots.len(),
            count,
            "아래 원형점선의 run 경계에서 원 누락·중복 방지"
        );
        for (i, dot) in dots.iter().enumerate() {
            near_print(attr(dot, "cy"), cy);
            assert!(
                (attr(dot, "cy") - cy).abs() / size < 0.01,
                "Print 원 중심의 위치 비율 보존"
            );
            near_print(attr(dot, "r"), radius);
            assert!(
                (attr(dot, "r") - radius).abs() / size < 0.002,
                "Print 원 크기 비율 보존"
            );
            near_print(attr(dot, "cx"), first + i as f64 * period);
        }
        near_print(attr(dots.last().unwrap(), "cx"), last);
    }
    let plain = fixture("");
    let decorated =
        fixture(r##"{"underlineType":"Bottom","underlineShape":6,"underlineColor":"#0000cc"}"##);
    let svg = decorated.render_page_svg_native(0).unwrap();
    let left = tags(&svg, "circle")
        .iter()
        .map(|dot| attr(dot, "cx") - attr(dot, "r"))
        .reduce(f64::min)
        .unwrap();
    assert!(
        left < body_clip(&plain).x,
        "첫 원은 본문 시작점 왼쪽으로 나간다"
    );
    assert!(
        body_clip(&decorated).x <= left,
        "아래 원형점선의 왼쪽 돌출을 본문 clip에 포함한다"
    );
    assert_eq!(body_clip(&decorated).y, body_clip(&plain).y);
    for ch in [0, TEXT.chars().count()] {
        assert_eq!(
            plain.get_cursor_rect_native(0, 0, ch).unwrap(),
            decorated.get_cursor_rect_native(0, 0, ch).unwrap()
        );
    }
    assert_round_run_boundaries(&doc, UnderlineType::Bottom);
}

fn assert_round_run_boundaries(doc: &HwpDocument, position: UnderlineType) {
    use rhwp::paint::{LayerBuilder, RenderProfile};
    use rhwp::renderer::layer_renderer::LayerRenderer;
    use rhwp::renderer::render_tree::PageRenderTree;
    use rhwp::renderer::svg_layer::SvgLayerRenderer;

    fn find(node: &RenderNode, position: UnderlineType) -> Option<RenderNode> {
        if matches!(node.node_type, RenderNodeType::TextLine(_))
            && node.children.iter().any(|child| {
                matches!(&child.node_type,
                RenderNodeType::TextRun(run) if run.style.underline == position)
            })
        {
            Some(node.clone())
        } else {
            node.children.iter().find_map(|child| find(child, position))
        }
    }
    fn circles(
        mut line: RenderNode,
        widths: &[f64],
        other: Option<UnderlineType>,
    ) -> Vec<(f64, f64, f64)> {
        let template = line.children[0].clone();
        line.children.clear();
        line.bbox.x = 0.0;
        line.bbox.width = widths.iter().sum();
        let mut x = 0.0;
        for (i, &width) in widths.iter().enumerate() {
            let mut child = template.clone();
            child.id += i as u32 * 100_000;
            child.bbox.x = x;
            child.bbox.width = width;
            let RenderNodeType::TextRun(run) = &mut child.node_type else {
                unreachable!()
            };
            run.text = "H".to_string();
            run.display_text = None;
            run.layout_positions = Some(vec![0.0, width]);
            run.is_para_end = false;
            run.is_line_break_end = false;
            if i == 1 {
                run.style.underline = other.unwrap_or(run.style.underline);
            }
            line.children.push(child);
            x += width;
        }
        let mut tree = PageRenderTree::new(0, 794.0, 1122.0);
        tree.root.children.push(line);
        let layer = LayerBuilder::new(RenderProfile::Print).build(&tree);
        let mut renderer = SvgLayerRenderer::new();
        renderer.render_page(&layer).unwrap();
        let mut dots: Vec<_> = tags(renderer.output(), "circle")
            .iter()
            .map(|t| (attr(t, "cx"), attr(t, "cy"), attr(t, "r")))
            .collect();
        dots.sort_by(|a, b| a.0.total_cmp(&b.0));
        dots
    }
    let line = find(&doc.build_page_render_tree(0).unwrap().root, position).unwrap();
    let whole = circles(line.clone(), &[64.0], None);
    // 9pt Print의 약 1.6px 주기 경계에서 run을 나눠도 원 소유와 위치는 같다.
    assert_eq!(whole.len(), 40);
    for n in [3, 8, 14, 22, 29] {
        let cut = n as f64 * 1.6;
        let split = circles(line.clone(), &[cut, 64.0 - cut], None);
        assert_eq!(split.len(), whole.len(), "주기 경계 {n}: 원 누락·중복 방지");
        for (a, b) in split.iter().zip(&whole) {
            assert!((a.0 - b.0).abs() < 1e-8 && a.1 == b.1 && a.2 == b.2);
        }
    }

    if position == UnderlineType::Bottom {
        for (first, second) in [
            (UnderlineType::Top, UnderlineType::Bottom),
            (UnderlineType::Bottom, UnderlineType::Top),
        ] {
            let mut mixed = line.clone();
            for child in &mut mixed.children {
                if let RenderNodeType::TextRun(run) = &mut child.node_type {
                    run.style.underline = first;
                }
            }
            // 같은 색·크기의 위·아래 전환도 새 시작점에서 원을 그린다.
            let dots = circles(mixed, &[3.0, 61.0], Some(second));
            let cy = dots
                .iter()
                .map(|dot| dot.1)
                .reduce(|a, b| {
                    if second == UnderlineType::Top {
                        a.min(b)
                    } else {
                        a.max(b)
                    }
                })
                .unwrap();
            let first_x = dots
                .iter()
                .filter(|dot| dot.1 == cy)
                .map(|dot| dot.0)
                .reduce(f64::min)
                .unwrap();
            assert!(
                (first_x - (3.0 - 0.08)).abs() < 1e-8,
                "위·아래 전환은 phase 초기화: {first_x}"
            );
        }
    }
}

#[test]
fn dashed_top_underline_phase_stays_with_its_contiguous_line() {
    assert_dashed_phase_boundaries(UnderlineType::Top);
}

fn assert_dashed_phase_boundaries(position: UnderlineType) {
    use rhwp::paint::{LayerBuilder, RenderProfile};
    use rhwp::renderer::render_tree::PageRenderTree;

    fn find(node: &RenderNode, position: UnderlineType) -> Option<RenderNode> {
        if matches!(node.node_type, RenderNodeType::TextLine(_))
            && node.children.iter().any(|child| {
                matches!(&child.node_type, RenderNodeType::TextRun(run)
                    if run.style.underline == position)
            })
        {
            return Some(node.clone());
        }
        node.children.iter().find_map(|child| find(child, position))
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
        position,
    )
    .unwrap();
    assert_eq!(
        original.children.len(),
        2,
        "영문·한글 run 경계가 있는 실제 원본"
    );
    let width = original.children[0].bbox.width;
    for change in [
        "none",
        "gap",
        "color",
        "position",
        "shape",
        "chain",
        "double-chain",
        "long",
        "round",
        "new-line",
    ] {
        let mut line = original.clone();
        match change {
            "gap" => line.children[1].bbox.x += 1.0,
            "color" => {
                let RenderNodeType::TextRun(run) = &mut line.children[1].node_type else {
                    unreachable!()
                };
                run.style.underline_color ^= 1;
            }
            "position" => {
                let RenderNodeType::TextRun(run) = &mut line.children[1].node_type else {
                    unreachable!()
                };
                run.style.underline = if position == UnderlineType::Top {
                    UnderlineType::Bottom
                } else {
                    UnderlineType::Top
                };
            }
            "shape" | "chain" | "double-chain" | "long" | "round" => {
                let RenderNodeType::TextRun(run) = &mut line.children[1].node_type else {
                    unreachable!()
                };
                run.style.underline_shape = match change {
                    "chain" => 3,
                    "double-chain" => 4,
                    "long" => 5,
                    "round" => 6,
                    _ => 2,
                };
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
            "gap" | "color" | "position" | "shape" | "chain" | "double-chain" | "long"
            | "round" => {
                vec![0.0, 0.0]
            }
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

#[test]
fn hancom_solid_bottom_underline_keeps_print_width_position_and_ink() {
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
    // 독립 한컴 Print PDF의 9/10/12/15/18/24/36/48pt 실측(em·96dpi 좌표)이다.
    for (pair, (offset, width, start, end, print_y, print_stroke)) in
        baselines.chunks_exact(2).zip([
            (
                0.16000145,
                0.04004371,
                113.37866667,
                153.83733333,
                163.50133333,
                0.48000000,
            ),
            (
                0.16860786,
                0.03618408,
                113.37866667,
                165.67066667,
                205.37466667,
                0.48000000,
            ),
            (
                0.17000634,
                0.04004371,
                113.37866667,
                176.06533333,
                255.08133333,
                0.64000000,
            ),
            (
                0.16796736,
                0.04004371,
                113.37866667,
                191.57733333,
                316.61333333,
                0.80000000,
            ),
            (
                0.17330870,
                0.04004371,
                113.37866667,
                207.40800000,
                391.09200000,
                0.96000000,
            ),
            (
                0.16998323,
                0.04000200,
                113.37866667,
                238.59200000,
                488.74533333,
                1.27866667,
            ),
            (
                0.16999333,
                0.04001591,
                113.37866667,
                301.11866667,
                632.90800000,
                1.91866667,
            ),
            (
                0.16749397,
                0.04002286,
                113.37866667,
                363.64400000,
                828.05466667,
                2.55866667,
            ),
        ])
    {
        let (baseline, size) = pair[1];
        let mut bottom: Vec<_> = lines
            .iter()
            .filter(|tag| tag.contains("stroke=\"#0000cc\""))
            .filter(|tag| (attr(tag, "y1") - baseline).abs() < size * 0.5)
            .collect();
        bottom.sort_by(|a, b| attr(a, "x1").total_cmp(&attr(b, "x1")));
        assert!(!bottom.is_empty(), "아래 실선 누락 방지");
        near_print(attr(bottom[0], "x1"), start);
        near_print(attr(bottom.last().unwrap(), "x2"), end);
        for run in bottom.windows(2) {
            assert!(
                (attr(run[0], "x2") - attr(run[1], "x1")).abs() < 1e-6,
                "run 경계 실선 연결"
            );
        }
        for line in bottom {
            let y = attr(line, "y1");
            let stroke = attr(line, "stroke-width");
            assert!(((y - baseline) / size - offset).abs() < 0.02);
            assert!(
                (stroke / size - width).abs() < 0.005,
                "크기별 Print 굵기: {stroke} / {size}"
            );
            near_print(y - stroke / 2.0, print_y - print_stroke / 2.0);
            near_print(y + stroke / 2.0, print_y + print_stroke / 2.0);
        }
    }
    let plain = fixture("");
    let underlined =
        fixture(r##"{"underlineType":"Bottom","underlineShape":0,"underlineColor":"#0000cc"}"##);
    let clip = body_clip(&underlined);
    let plain_clip = body_clip(&plain);
    assert_eq!(
        (clip.x, clip.y, clip.width, clip.height),
        (
            plain_clip.x,
            plain_clip.y,
            plain_clip.width,
            plain_clip.height
        ),
        "글줄 상자 보존"
    );
    for pos in [0, TEXT.chars().count()] {
        assert_eq!(
            underlined.get_cursor_rect_native(0, 0, pos).unwrap(),
            plain.get_cursor_rect_native(0, 0, pos).unwrap()
        );
    }
}

#[test]
fn hancom_dashed_bottom_underline_keeps_print_geometry_across_font_runs() {
    assert_dashed_phase_boundaries(UnderlineType::Bottom);
    // 같은 독립 Print PDF의 9~48pt 아래 파선 실측(em·96dpi px)이다.
    assert_pattern_probe(
        "top-probe-shape-1",
        UnderlineType::Bottom,
        [
            (0.16000, 0.04004, 3.35867, 5.27733, 153.67733, 8),
            (0.16861, 0.03618, 3.83867, 6.07733, 165.67067, 9),
            (0.17001, 0.04004, 4.47733, 7.35600, 176.06533, 9),
            (0.16797, 0.04004, 5.75733, 9.27600, 191.57733, 9),
            (0.17331, 0.04004, 6.87600, 11.03467, 207.40800, 9),
            (0.16998, 0.04000, 9.11600, 14.55200, 238.59200, 9),
            (0.16999, 0.04002, 13.59333, 21.90800, 301.11867, 9),
            (0.16749, 0.04002, 18.23067, 29.10533, 363.64400, 9),
        ],
        None,
    );
    let plain = fixture("");
    let underlined =
        fixture(r##"{"underlineType":"Bottom","underlineShape":1,"underlineColor":"#0000cc"}"##);
    let clip = body_clip(&underlined);
    let plain_clip = body_clip(&plain);
    assert_eq!(
        (clip.x, clip.y, clip.width, clip.height),
        (
            plain_clip.x,
            plain_clip.y,
            plain_clip.width,
            plain_clip.height
        ),
        "글줄 상자 보존"
    );
    for pos in [0, TEXT.chars().count()] {
        assert_eq!(
            underlined.get_cursor_rect_native(0, 0, pos).unwrap(),
            plain.get_cursor_rect_native(0, 0, pos).unwrap()
        );
    }
}
