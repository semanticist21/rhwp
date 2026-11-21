//! 도형 캡션 글 편집(셀 65534), 캡션 너비 변경 뒤 다시 줄 나누기, '여백 부분까지 너비 확대',
//! 글자처럼 취급 그림과 셀 안 그림의 캡션 자리를 한컴 12.30 기준으로 검사한다.

use crate::document_core::{DocumentCore, TABLE_CAPTION_CELL_SENTINEL as CAPTION};
use crate::model::control::Control;
use crate::renderer::layout::CellContext;
use crate::renderer::render_tree::{BoundingBox, RenderNode, RenderNodeType};

fn blank() -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().expect("빈 문서");
    core
}

fn json_usize(json: &str, key: &str) -> usize {
    let value: serde_json::Value = serde_json::from_str(json).expect("JSON");
    value[key].as_u64().expect(key) as usize
}

fn png() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
        0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
}

/// 캡션 글줄 상자와 첫 글자 조각의 셀 문맥.
fn caption_lines(core: &DocumentCore) -> Vec<(BoundingBox, Option<CellContext>)> {
    fn walk(node: &RenderNode, out: &mut Vec<(BoundingBox, Option<CellContext>)>) {
        if let RenderNodeType::TextLine(line) = &node.node_type {
            if line.caption_owner.is_some() {
                let ctx = node
                    .children
                    .iter()
                    .find_map(|child| match &child.node_type {
                        RenderNodeType::TextRun(run) => Some(run.cell_context.clone()),
                        _ => None,
                    });
                out.push((node.bbox, ctx.flatten()));
            }
        }
        for child in &node.children {
            walk(child, out);
        }
    }
    let tree = core.build_page_tree(0).expect("쪽 트리");
    let mut out = Vec::new();
    walk(&tree.root, &mut out);
    out
}

fn image_boxes(core: &DocumentCore) -> Vec<BoundingBox> {
    fn walk(node: &RenderNode, out: &mut Vec<BoundingBox>) {
        if matches!(node.node_type, RenderNodeType::Image(_)) {
            out.push(node.bbox);
        }
        for child in &node.children {
            walk(child, out);
        }
    }
    let tree = core.build_page_tree(0).expect("쪽 트리");
    let mut out = Vec::new();
    walk(&tree.root, &mut out);
    out
}

fn floating_rectangle(core: &mut DocumentCore) -> (usize, usize) {
    let result = core
        .create_shape_control_native(
            0,
            0,
            0,
            9000,
            6000,
            20000,
            20000,
            false,
            "InFrontOfText",
            "rectangle",
            false,
            false,
            &[],
        )
        .expect("사각형");
    (
        json_usize(&result, "paraIdx"),
        json_usize(&result, "controlIdx"),
    )
}

fn floating_picture(core: &mut DocumentCore, props: &str) -> (usize, usize) {
    let result = core
        .insert_picture_native(
            0,
            0,
            0,
            &[],
            &png(),
            9600,
            9600,
            1,
            1,
            "png",
            "",
            None,
            None,
        )
        .expect("그림");
    let (para, ctrl) = (
        json_usize(&result, "paraIdx"),
        json_usize(&result, "controlIdx"),
    );
    core.set_picture_properties_native(
        0,
        para,
        ctrl,
        r#"{"treatAsChar":false,"textWrap":"InFrontOfText","horzRelTo":"Paper","vertRelTo":"Paper","horzOffset":8000,"vertOffset":12000}"#,
    )
    .expect("띄우기");
    core.set_picture_properties_native(0, para, ctrl, props)
        .expect("캡션");
    (para, ctrl)
}

fn picture(core: &DocumentCore, para: usize, ctrl: usize) -> &crate::model::image::Picture {
    match &core.document.sections[0].paragraphs[para].controls[ctrl] {
        Control::Picture(picture) => picture,
        other => panic!("그림이 아님: {other:?}"),
    }
}

#[test]
fn shape_caption_text_is_edited_through_the_caption_cell() {
    let mut core = blank();
    let (para, ctrl) = floating_rectangle(&mut core);
    core.set_shape_properties_native(
        0,
        para,
        ctrl,
        r#"{"hasCaption":true,"captionDirection":"Bottom","captionVertAlign":"Top","captionWidth":8504,"captionSpacing":850}"#,
    )
    .expect("캡션");

    // 한컴 12.30처럼 도형 캡션 번호도 "그림 N"이고 번호 뒤에 마침표가 없다.
    let Control::Shape(shape) = &core.document.sections[0].paragraphs[para].controls[ctrl] else {
        panic!("도형이 아님");
    };
    let caption = DocumentCore::shape_caption_ref(shape).expect("캡션");
    assert!(caption.paragraphs[0]
        .controls
        .iter()
        .any(|control| matches!(
            control,
            Control::AutoNumber(number) if number.suffix_char != '.'
        )));

    let end = core
        .get_cell_paragraph_length_native(0, para, ctrl, CAPTION, 0)
        .expect("캡션 길이");
    core.insert_text_in_cell_native(0, para, ctrl, CAPTION, 0, end, " 수달")
        .expect("캡션 입력");
    assert_eq!(
        core.get_text_in_cell_native(0, para, ctrl, CAPTION, 0, end, 3)
            .expect("캡션 글"),
        " 수달"
    );
    core.split_paragraph_in_cell_native(0, para, ctrl, CAPTION, 0, end + 1, None)
        .expect("캡션 문단 나누기");
    assert_eq!(
        core.get_cell_paragraph_count_native(0, para, ctrl, CAPTION)
            .expect("캡션 문단 수"),
        2
    );
    let second = core
        .resolve_paragraph_by_path(0, para, &[(ctrl, CAPTION, 1)])
        .expect("경로로 둘째 캡션 문단");
    assert_eq!(second.text, "수달");

    // 캡션 글줄은 셀 65534 주소를 가지고, 누르면 캡션 캐럿이 된다.
    let lines = caption_lines(&core);
    assert_eq!(lines.len(), 2);
    for (pi, (_, ctx)) in lines.iter().enumerate() {
        let last = ctx
            .as_ref()
            .and_then(|ctx| ctx.path.last())
            .expect("셀 문맥");
        assert_eq!(
            (last.control_index, last.cell_index, last.cell_para_index),
            (ctrl, CAPTION, pi)
        );
    }
    let (bbox, _) = &lines[1];
    let hit = core
        .hit_test_native(0, bbox.x + 2.0, bbox.y + bbox.height / 2.0)
        .expect("누르기");
    let hit: serde_json::Value = serde_json::from_str(&hit).expect("JSON");
    assert_eq!(hit["cellIndex"], CAPTION);
    assert_eq!(hit["controlIndex"], ctrl);
    assert_eq!(hit["cellParaIndex"], 1);
    core.get_cursor_rect_in_cell_native(0, para, ctrl, CAPTION, 1, 1)
        .expect("캡션 캐럿 자리");
}

#[test]
fn narrowing_a_side_caption_rewraps_its_text() {
    let mut core = blank();
    let (para, ctrl) = floating_picture(
        &mut core,
        r#"{"hasCaption":true,"captionDirection":"Right","captionVertAlign":"Top","captionWidth":8504,"captionSpacing":850}"#,
    );
    let end = core
        .get_cell_paragraph_length_native(0, para, ctrl, 0, 0)
        .expect("캡션 길이");
    core.insert_text_in_cell_native(
        0,
        para,
        ctrl,
        0,
        0,
        end,
        " 오른쪽 캡션은 너비를 줄이면 글을 다시 줄바꿈합니다.",
    )
    .expect("캡션 입력");
    let wide = picture(&core, para, ctrl)
        .caption
        .as_ref()
        .unwrap()
        .paragraphs[0]
        .line_segs
        .len();

    core.set_picture_properties_native(0, para, ctrl, r#"{"hasCaption":true,"captionWidth":2834}"#)
        .expect("캡션 크기");
    let caption = picture(&core, para, ctrl).caption.as_ref().unwrap();
    assert!(caption.paragraphs[0].line_segs.len() > wide);
    // 한컴은 줄 나눈 폭을 캡션 글 최대 폭으로 저장한다.
    assert_eq!(caption.max_width, 2834);
    let lines = caption_lines(&core);
    assert_eq!(lines.len(), caption.paragraphs[0].line_segs.len());
    assert!(lines
        .iter()
        .all(|(bbox, _)| (bbox.width - 37.8).abs() < 0.2));

    // 한컴 12.30처럼 옆 캡션은 개체 높이에서 잘라 그린다.
    let image = image_boxes(&core)[0];
    fn clips(node: &RenderNode, out: &mut Vec<Option<f64>>) {
        if let RenderNodeType::TextLine(line) = &node.node_type {
            if line.caption_owner.is_some() {
                out.push(node.paint_clip.as_ref().map(|paths| {
                    paths[0]
                        .commands
                        .iter()
                        .filter_map(|command| match command {
                            crate::model::text_clip::ClipCommand::Line(_, y) => Some(*y),
                            _ => None,
                        })
                        .fold(f64::MIN, f64::max)
                }));
            }
        }
        for child in &node.children {
            clips(child, out);
        }
    }
    let mut bottoms = Vec::new();
    clips(
        &core.build_page_tree(0).expect("쪽 트리").root,
        &mut bottoms,
    );
    assert!(!bottoms.is_empty());
    for bottom in bottoms {
        let bottom = bottom.expect("옆 캡션 자름");
        assert!(
            (bottom - (image.y + image.height)).abs() < 0.5,
            "{bottom} {image:?}"
        );
    }
}

#[test]
fn include_margin_widens_a_bottom_caption_to_the_outer_margins() {
    let mut core = blank();
    let (para, ctrl) = floating_picture(
        &mut core,
        r#"{"outerMarginLeft":1500,"outerMarginRight":1500,"hasCaption":true,"captionDirection":"Bottom","captionVertAlign":"Top","captionWidth":8504,"captionSpacing":850}"#,
    );
    let image = image_boxes(&core)[0];
    let (plain, _) = caption_lines(&core).remove(0);
    assert!((plain.x - image.x).abs() < 0.1);
    assert!((plain.width - image.width).abs() < 0.1);

    core.set_picture_properties_native(
        0,
        para,
        ctrl,
        r#"{"hasCaption":true,"captionIncludeMargin":true}"#,
    )
    .expect("여백까지 너비 확대");
    let (wide, _) = caption_lines(&core).remove(0);
    // 1500 HWPUNIT = 20px(96dpi). 캡션이 왼쪽 바깥 여백부터 시작해 좌우 여백만큼 넓다.
    assert!((wide.x - (image.x - 20.0)).abs() < 0.1);
    assert!((wide.width - (image.width + 40.0)).abs() < 0.1);
    assert_eq!(
        picture(&core, para, ctrl)
            .caption
            .as_ref()
            .unwrap()
            .max_width,
        9600 + 3000
    );
}

#[test]
fn inline_picture_caption_starts_under_the_picture() {
    let mut core = blank();
    core.insert_text_native(0, 0, 0, "글자처럼 그림: 앞")
        .expect("본문");
    let result = core
        .insert_picture_native(
            0,
            0,
            8,
            &[],
            &png(),
            6000,
            6000,
            1,
            1,
            "png",
            "",
            None,
            None,
        )
        .expect("그림");
    let ctrl = json_usize(&result, "controlIdx");
    core.set_picture_properties_native(0, 0, ctrl, r#"{"treatAsChar":true}"#)
        .expect("글자처럼 취급");
    core.set_picture_properties_native(
        0,
        0,
        ctrl,
        r#"{"hasCaption":true,"captionDirection":"Bottom","captionVertAlign":"Top","captionWidth":8504,"captionSpacing":850}"#,
    )
    .expect("캡션");
    let image = image_boxes(&core)[0];
    let (line, _) = caption_lines(&core).remove(0);
    assert!(image.x > 100.0, "그림이 글 뒤에 있어야 한다: {image:?}");
    assert!((line.x - image.x).abs() < 0.1, "{line:?} {image:?}");
    assert!((line.width - image.width).abs() < 0.1, "{line:?} {image:?}");
}

#[test]
fn cell_picture_caption_lines_address_the_caption() {
    let mut core = blank();
    let table = core.create_table_native(0, 0, 0, 1, 2).expect("표");
    let table_ctrl = json_usize(&table, "controlIdx");
    let table_para = json_usize(&table, "paraIdx");
    let cell = [(table_ctrl, 0usize, 0usize)];
    let picture = core
        .insert_picture_in_cell_native(
            0,
            table_para,
            0,
            &cell,
            &png(),
            6000,
            6000,
            1,
            1,
            "png",
            "",
            None,
            None,
        )
        .expect("셀 그림");
    let inner = json_usize(&picture, "controlIdx");
    let path = format!(r#"[{{"controlIndex":{table_ctrl},"cellIndex":0,"cellParaIndex":0}}]"#);
    core.set_cell_picture_properties_by_path_native(
        0,
        table_para,
        &path,
        inner,
        r#"{"hasCaption":true,"captionDirection":"Bottom","captionVertAlign":"Top","captionWidth":8504,"captionSpacing":850}"#,
    )
    .expect("셀 그림 캡션");
    let lines = caption_lines(&core);
    assert!(!lines.is_empty());
    let ctx = lines[0].1.as_ref().expect("셀 문맥");
    let steps: Vec<_> = ctx
        .path
        .iter()
        .map(|entry| (entry.control_index, entry.cell_index, entry.cell_para_index))
        .collect();
    assert_eq!(steps, vec![(table_ctrl, 0, 0), (inner, 0, 0)]);

    // 한컴 12.30처럼 셀이 아래 캡션까지 커져 캡션 글줄이 셀 안에 든다.
    let cell_box = first_cell_box(&core);
    let last = lines.last().expect("캡션 글줄").0;
    assert!(
        last.y + last.height <= cell_box.y + cell_box.height + 0.5,
        "{last:?} {cell_box:?}"
    );
    // 캡션 글줄을 누르면 그 그림 캡션 자리가 된다.
    let hit = core
        .hit_test_native(0, last.x + 2.0, last.y + last.height / 2.0)
        .expect("누르기");
    let hit: serde_json::Value = serde_json::from_str(&hit).expect("JSON");
    let path = hit["cellPath"].as_array().expect("셀 경로");
    assert_eq!(path.len(), 2, "{hit}");
    assert_eq!(path[1]["controlIndex"], inner);
}

fn first_cell_box(core: &DocumentCore) -> BoundingBox {
    fn walk(node: &RenderNode) -> Option<BoundingBox> {
        if matches!(node.node_type, RenderNodeType::TableCell(_)) {
            return Some(node.bbox);
        }
        node.children.iter().find_map(walk)
    }
    walk(&core.build_page_tree(0).expect("쪽 트리").root).expect("셀")
}

#[test]
fn header_picture_caption_line_hits_its_caption() {
    header_picture_caption(false, false);
}

/// 한컴 12.30은 머리말에 넣은 그림을 글자처럼 취급하고 그 아래 캡션을 그림 바로 아래에 그린다.
#[test]
fn header_inline_picture_caption_is_drawn_and_hits_its_caption() {
    header_picture_caption(true, false);
}

/// 한컴 12.30처럼 머리말 그림에도 캡션을 새로 단다.
#[test]
fn header_picture_gets_a_new_caption() {
    header_picture_caption(true, true);
}

fn header_picture_caption(treat_as_char: bool, new_caption: bool) {
    const CAPTION: &str = r#""hasCaption":true,"captionDirection":"Bottom","captionVertAlign":"Top","captionWidth":8504,"captionSpacing":850"#;
    let mut core = blank();
    let (para, ctrl) = floating_picture(
        &mut core,
        &if new_caption {
            format!(r#"{{"treatAsChar":{treat_as_char}}}"#)
        } else {
            format!(r#"{{"treatAsChar":{treat_as_char},{CAPTION}}}"#)
        },
    );
    let picture = core.document.sections[0].paragraphs[para].controls[ctrl].clone();
    core.create_header_footer_native(0, true, 0)
        .expect("머리말");
    let info: serde_json::Value = serde_json::from_str(
        &core
            .get_header_footer_native(0, true, 0)
            .expect("머리말 정보"),
    )
    .expect("JSON");
    let host = info["paraIndex"].as_u64().expect("paraIndex") as usize;
    let hf = info["controlIndex"].as_u64().expect("controlIndex") as usize;
    // 한컴 12.30에서 머리말에 넣은 그림처럼, 캡션 달린 그림을 머리말 첫 문단 맨 앞 컨트롤로 둔다.
    let Control::Header(header) = &mut core.document.sections[0].paragraphs[host].controls[hf]
    else {
        panic!("머리말이 아니다");
    };
    let first = &mut header.paragraphs[0];
    first.controls.insert(0, picture);
    first.shift_for_inline_control_insert(0, 0);
    first.char_count += 8;
    core.document.sections[0].raw_stream = None;
    core.mark_section_dirty(0);
    core.paginate_if_needed();
    if new_caption {
        core.set_header_footer_picture_properties_native(
            0,
            host,
            hf,
            0,
            0,
            &format!("{{{CAPTION}}}"),
        )
        .expect("머리말 그림 캡션 넣기");
    }

    // 머리말 캡션 글줄의 셀 문맥은 머리말 표 칸처럼 머리말 문단이 뿌리인 [(그림, 0, 0)]이다.
    // 머리말 글줄은 본문 캡션 소유자(caption_owner)를 싣지 않으므로 글자 조각의 셀 문맥으로 찾는다.
    fn header_caption(node: &RenderNode) -> Option<(BoundingBox, CellContext)> {
        if matches!(node.node_type, RenderNodeType::TextLine(_)) {
            let ctx = node
                .children
                .iter()
                .find_map(|child| match &child.node_type {
                    RenderNodeType::TextRun(run) => run.cell_context.clone(),
                    _ => None,
                });
            if let Some(ctx) = ctx.filter(|ctx| ctx.path.len() == 1) {
                return Some((node.bbox, ctx));
            }
        }
        node.children.iter().find_map(header_caption)
    }
    let tree = core.build_page_tree(0).expect("쪽 트리");
    let header_node = tree
        .root
        .children
        .iter()
        .find(|child| matches!(child.node_type, RenderNodeType::Header))
        .expect("머리말 노드");
    let (line, ctx) = header_caption(header_node).expect("머리말 캡션 글줄");
    assert_eq!(ctx.parent_para_index, 0);
    let steps: Vec<_> = ctx
        .path
        .iter()
        .map(|entry| (entry.control_index, entry.cell_index, entry.cell_para_index))
        .collect();
    assert_eq!(steps, vec![(0, 0, 0)]);

    // 머리말 편집에서 그 글줄을 누르면 그림 캡션 자리가 된다.
    let hit = core
        .hit_test_in_header_footer_target_native(
            0,
            0,
            true,
            0,
            line.x + 2.0,
            line.y + line.height / 2.0,
        )
        .expect("누르기");
    let hit: serde_json::Value = serde_json::from_str(&hit).expect("JSON");
    let path = hit["cellPath"].as_array().expect("셀 경로");
    assert_eq!(path.len(), 2, "{hit}");
    assert_eq!(hit["parentParaIndex"], host);
    assert_eq!(path[0]["controlIndex"], hf);
    assert_eq!(path[1]["controlIndex"], 0);
}

/// 한컴 12.30은 캡션 글에 캡션 달린 그림을 넣을 수 있다. HWP로 저장해 다시 열어도 안쪽 그림은
/// 그림과 그 캡션으로 남는다(바깥 그림의 캡션이 안쪽 그림의 SHAPE_COMPONENT 앞에서 잘리면 안쪽
/// 그림이 사각형 글상자가 되고, 한컴은 그 파일을 열지 못한다).
#[test]
fn captioned_picture_inside_a_caption_survives_hwp_round_trip() {
    let caption = r#"{"hasCaption":true,"captionDirection":"Bottom","captionVertAlign":"Top","captionWidth":8504,"captionSpacing":850}"#;
    let mut core = blank();
    let (para, ctrl) = floating_picture(&mut core, caption);
    let mut inner = core.document.sections[0].paragraphs[para].controls[ctrl].clone();
    if let Control::Picture(pic) = &mut inner {
        pic.common.treat_as_char = true;
    }
    let Control::Picture(outer) = &mut core.document.sections[0].paragraphs[para].controls[ctrl]
    else {
        panic!("그림이 아니다");
    };
    let first = &mut outer.caption.as_mut().expect("캡션").paragraphs[0];
    let end = first.text.chars().count();
    let at = first.controls.len();
    first.controls.push(inner);
    first.shift_for_inline_control_insert(at, end);
    first.char_count += 8;
    core.document.sections[0].raw_stream = None;

    let reopened = DocumentCore::from_bytes(&core.export_hwp_native().expect("HWP")).expect("열기");
    let Control::Picture(outer) = &reopened.document.sections[0].paragraphs[para].controls[ctrl]
    else {
        panic!("바깥 그림이 아니다");
    };
    let controls = &outer.caption.as_ref().expect("바깥 캡션").paragraphs[0].controls;
    let inner = controls
        .iter()
        .find_map(|control| match control {
            Control::Picture(pic) => Some(pic),
            _ => None,
        })
        .expect("캡션 안 그림");
    assert!(inner.caption.is_some(), "안쪽 그림 캡션");
}

/// 머리말 첫 문단 맨 앞에 개체를 넣고 (머리말 컨트롤을 품은 본문 문단, 머리말 컨트롤 번호)를 준다.
fn put_in_header(core: &mut DocumentCore, control: Control) -> (usize, usize) {
    core.create_header_footer_native(0, true, 0)
        .expect("머리말");
    let info: serde_json::Value = serde_json::from_str(
        &core
            .get_header_footer_native(0, true, 0)
            .expect("머리말 정보"),
    )
    .expect("JSON");
    let host = info["paraIndex"].as_u64().expect("paraIndex") as usize;
    let hf = info["controlIndex"].as_u64().expect("controlIndex") as usize;
    let Control::Header(header) = &mut core.document.sections[0].paragraphs[host].controls[hf]
    else {
        panic!("머리말이 아니다");
    };
    let first = &mut header.paragraphs[0];
    first.controls.insert(0, control);
    first.shift_for_inline_control_insert(0, 0);
    first.char_count += 8;
    core.document.sections[0].raw_stream = None;
    core.mark_section_dirty(0);
    core.paginate_if_needed();
    (host, hf)
}

/// 머리말 노드 안에서 셀 문맥 마디가 하나인 캡션 글줄(상자, 문맥).
fn header_caption_line(core: &DocumentCore) -> Option<(BoundingBox, CellContext)> {
    fn walk(node: &RenderNode) -> Option<(BoundingBox, CellContext)> {
        if matches!(node.node_type, RenderNodeType::TextLine(_)) {
            let ctx = node
                .children
                .iter()
                .find_map(|child| match &child.node_type {
                    RenderNodeType::TextRun(run) => run.cell_context.clone(),
                    _ => None,
                });
            if let Some(ctx) = ctx.filter(|ctx| ctx.path.len() == 1) {
                return Some((node.bbox, ctx));
            }
        }
        node.children.iter().find_map(walk)
    }
    let tree = core.build_page_tree(0).expect("쪽 트리");
    let header = tree
        .root
        .children
        .iter()
        .find(|child| matches!(child.node_type, RenderNodeType::Header))
        .expect("머리말 노드");
    walk(header)
}

/// 한컴 12.30처럼 머리말 도형에도 캡션을 달고, 그 캡션 글을 머리말 경로로 누르고 고친다.
#[test]
fn header_shape_caption_is_addressed_hit_and_edited() {
    let mut core = blank();
    let (para, ctrl) = floating_rectangle(&mut core);
    let rect = core.document.sections[0].paragraphs[para].controls[ctrl].clone();
    let (host, hf) = put_in_header(&mut core, rect);
    let path_json = format!(r#"[{{"controlIndex":{hf},"cellIndex":0,"cellParaIndex":0}}]"#);
    core.set_cell_shape_properties_by_path_native(
        0,
        host,
        &path_json,
        0,
        r#"{"hasCaption":true,"captionDirection":"Bottom","captionVertAlign":"Top","captionWidth":8504,"captionSpacing":850}"#,
    )
    .expect("머리말 도형 캡션 넣기");

    // 캡션 글줄은 머리말 문단이 뿌리인 [(도형, 65534, 0)] 문맥을 갖는다.
    let (line, ctx) = header_caption_line(&core).expect("머리말 도형 캡션 글줄");
    assert_eq!(ctx.parent_para_index, 0);
    let steps: Vec<_> = ctx
        .path
        .iter()
        .map(|entry| (entry.control_index, entry.cell_index, entry.cell_para_index))
        .collect();
    assert_eq!(steps, vec![(0, CAPTION, 0)]);

    // 머리말 편집에서 그 글줄을 누르면 [(머리말, 0, 0), (도형, 65534, 0)] 캡션 자리가 된다.
    let hit = core
        .hit_test_in_header_footer_target_native(
            0,
            0,
            true,
            0,
            line.x + 2.0,
            line.y + line.height / 2.0,
        )
        .expect("누르기");
    let hit: serde_json::Value = serde_json::from_str(&hit).expect("JSON");
    let path = hit["cellPath"].as_array().expect("셀 경로");
    assert_eq!(hit["parentParaIndex"], host, "{hit}");
    assert_eq!(path.len(), 2, "{hit}");
    assert_eq!(path[0]["controlIndex"], hf);
    assert_eq!(path[1]["controlIndex"], 0);
    assert_eq!(path[1]["cellIndex"], CAPTION);

    // 그 경로로 캐럿 자리를 얻고 글을 넣는다.
    let caption_path = [(hf, 0, 0), (0, CAPTION, 0)];
    let caption_json = format!(
        r#"[{{"controlIndex":{hf},"cellIndex":0,"cellParaIndex":0}},{{"controlIndex":0,"cellIndex":{CAPTION},"cellParaIndex":0}}]"#
    );
    let end = core
        .resolve_paragraph_by_path(0, host, &caption_path)
        .expect("캡션 문단")
        .char_count as usize
        - 1;
    core.get_cursor_rect_by_path_native(0, host, &caption_json, end)
        .expect("머리말 도형 캡션 캐럿");
    core.insert_text_in_cell_by_path(0, host, &caption_path, end, "수달")
        .expect("머리말 도형 캡션 입력");
    assert!(core
        .resolve_paragraph_by_path(0, host, &caption_path)
        .expect("캡션 문단")
        .text
        .ends_with("수달"));
}

/// 한컴 12.30은 머리말 글자처럼 취급 그림의 위 캡션을 줄 위끝에 두고 그림을 캡션 높이와 간격만큼 내린다.
#[test]
fn header_inline_picture_top_caption_sits_above_the_picture() {
    let mut core = blank();
    let (para, ctrl) = floating_picture(
        &mut core,
        r#"{"treatAsChar":true,"hasCaption":true,"captionDirection":"Bottom","captionVertAlign":"Top","captionWidth":8504,"captionSpacing":850}"#,
    );
    let picture = core.document.sections[0].paragraphs[para].controls[ctrl].clone();
    let (host, hf) = put_in_header(&mut core, picture);
    // 한컴에서처럼 그림 앞에 글을 둔다. 글줄은 그림 상자 위끝을 줄 위끝보다 위에 놓을 수 있다.
    core.insert_text_in_header_footer_native(0, true, 0, 0, 0, "머리")
        .expect("머리말 글");
    // 앱처럼 머리말 그림 setter로 캡션을 위로 옮기면 그 머리말 줄이 캡션까지 품는다.
    core.set_header_footer_picture_properties_native(
        0,
        host,
        hf,
        0,
        0,
        r#"{"hasCaption":true,"captionDirection":"Top","captionVertAlign":"Top"}"#,
    )
    .expect("위 캡션으로 옮기기");

    let (line, ctx) = header_caption_line(&core).expect("머리말 위 캡션 글줄");
    assert_eq!(ctx.path[0].control_index, 0);
    let tree = core.build_page_tree(0).expect("쪽 트리");
    let header = tree
        .root
        .children
        .iter()
        .find(|child| matches!(child.node_type, RenderNodeType::Header))
        .expect("머리말 노드");
    fn image(node: &RenderNode) -> Option<BoundingBox> {
        if matches!(node.node_type, RenderNodeType::Image(_)) {
            return Some(node.bbox);
        }
        node.children.iter().find_map(image)
    }
    let image = image(header).expect("머리말 그림");
    // 캡션은 그림이 든 글줄 위끝에서 시작하고, 그 줄의 글 기준선은 그림 아래 끝이다.
    fn image_line(node: &RenderNode) -> Option<&RenderNode> {
        if matches!(node.node_type, RenderNodeType::TextLine(_))
            && node
                .children
                .iter()
                .any(|child| matches!(child.node_type, RenderNodeType::Image(_)))
        {
            return Some(node);
        }
        node.children.iter().find_map(image_line)
    }
    let text_line = image_line(header).expect("그림이 든 머리말 글줄");
    assert!(
        line.y >= text_line.bbox.y - 0.5,
        "캡션 {line:?} 글줄 {:?}",
        text_line.bbox
    );
    assert!(
        image.y + image.height <= text_line.bbox.y + text_line.bbox.height + 0.5,
        "그림 {image:?} 글줄 {:?}",
        text_line.bbox
    );
    // 캡션 글줄 아래 3mm(850HU) 간격 뒤에 그림이 온다.
    let gap = crate::renderer::hwpunit_to_px(850, core.dpi);
    assert!(
        image.y >= line.y + line.height + gap - 1.0,
        "캡션 {line:?} 그림 {image:?}"
    );
}

/// 한컴 12.30은 각주 안 글자처럼 취급 그림도 골라 캡션을 단다. 캡션은 각주 줄 높이에 들어 그 그림 아래에
/// 그려지고, 그 글줄은 본문 문단이 뿌리인 [(각주, 0, 각주 문단), (그림, 0, 캡션 문단)] 주소로 누르고 고친다.
#[test]
fn footnote_inline_picture_caption_is_drawn_addressed_hit_and_edited() {
    let mut core = blank();
    let (para, ctrl) = floating_picture(&mut core, r#"{"treatAsChar":true}"#);
    let picture = core.document.sections[0].paragraphs[para].controls[ctrl].clone();
    let note: serde_json::Value =
        serde_json::from_str(&core.insert_footnote_native(0, para, 0).expect("각주"))
            .expect("JSON");
    let note = note["controlIdx"].as_u64().expect("controlIdx") as usize;
    core.insert_text_in_footnote_native(0, para, note, 0, 0, "각주")
        .expect("각주 글");
    // 한컴에서 각주에 넣은 그림처럼 각주 첫 문단 끝에 글자처럼 취급 그림을 둔다.
    let inner = {
        let Control::Footnote(footnote) =
            &mut core.document.sections[0].paragraphs[para].controls[note]
        else {
            panic!("각주가 아니다");
        };
        let first = &mut footnote.paragraphs[0];
        let end = first.text.chars().count();
        let at = first.controls.len();
        first.controls.push(picture);
        first.shift_for_inline_control_insert(at, end);
        first.char_count += 8;
        at
    };
    core.document.sections[0].raw_stream = None;
    core.reflow_footnote_paragraph(0, para, note, 0);
    core.mark_section_dirty(0);
    core.paginate_if_needed();

    let note_path = format!(r#"[{{"controlIndex":{note},"cellIndex":0,"cellParaIndex":0}}]"#);
    core.set_cell_picture_properties_by_path_native(
        0,
        para,
        &note_path,
        inner,
        r#"{"hasCaption":true,"captionDirection":"Bottom","captionVertAlign":"Top","captionWidth":8504,"captionSpacing":850}"#,
    )
    .expect("각주 그림 캡션 넣기");

    // 쪽 개체 목록은 각주 그림을 각주 경로로 알린다.
    let layout: serde_json::Value =
        serde_json::from_str(&core.get_page_control_layout_native(0).expect("개체 목록"))
            .expect("JSON");
    let image = layout["controls"]
        .as_array()
        .expect("개체")
        .iter()
        .find(|c| c["type"] == "image" && c["cellPath"].is_array())
        .expect("각주 그림");
    assert_eq!(image["paraIdx"], para, "{image}");
    assert_eq!(image["controlIdx"], inner);
    assert_eq!(image["cellPath"][0]["controlIndex"], note);

    // 캡션 글줄은 각주 영역에 그 그림 아래로 그려지고 본문 문단 뿌리 경로를 갖는다.
    let tree = core.build_page_tree(0).expect("쪽 트리");
    let area = tree
        .root
        .children
        .iter()
        .find(|child| matches!(child.node_type, RenderNodeType::FootnoteArea))
        .expect("각주 영역");
    fn caption_line(node: &RenderNode) -> Option<(BoundingBox, CellContext)> {
        if matches!(node.node_type, RenderNodeType::TextLine(_)) {
            let ctx = node
                .children
                .iter()
                .find_map(|child| match &child.node_type {
                    RenderNodeType::TextRun(run) => run.cell_context.clone(),
                    _ => None,
                });
            if let Some(ctx) = ctx.filter(|ctx| ctx.path.len() == 2) {
                return Some((node.bbox, ctx));
            }
        }
        node.children.iter().find_map(caption_line)
    }
    fn image_box(node: &RenderNode) -> Option<BoundingBox> {
        if matches!(node.node_type, RenderNodeType::Image(_)) {
            return Some(node.bbox);
        }
        node.children.iter().find_map(image_box)
    }
    let (line, ctx) = caption_line(area).expect("각주 캡션 글줄");
    let picture_box = image_box(area).expect("각주 그림");
    assert!(
        line.y >= picture_box.y + picture_box.height,
        "{line:?} {picture_box:?}"
    );
    assert!(
        line.y + line.height <= area.bbox.y + area.bbox.height + 0.5,
        "각주 영역 안"
    );
    assert_eq!(ctx.parent_para_index, para);
    let steps: Vec<_> = ctx
        .path
        .iter()
        .map(|entry| (entry.control_index, entry.cell_index, entry.cell_para_index))
        .collect();
    assert_eq!(steps, vec![(note, 0, 0), (inner, 0, 0)]);

    // 각주에서 그 글줄을 누르면 캡션 자리가 된다.
    let hit: serde_json::Value = serde_json::from_str(
        &core
            .hit_test_in_footnote_native(0, line.x + 2.0, line.y + line.height / 2.0)
            .expect("누르기"),
    )
    .expect("JSON");
    assert_eq!(hit["caption"], true, "{hit}");
    assert_eq!(hit["parentParaIndex"], para);
    assert_eq!(hit["cellPath"][1]["controlIndex"], inner);

    // 그 경로로 캐럿 자리를 얻고 캡션 글을 친다.
    let caption_path = [(note, 0, 0), (inner, 0, 0)];
    let caption_json = format!(
        r#"[{{"controlIndex":{note},"cellIndex":0,"cellParaIndex":0}},{{"controlIndex":{inner},"cellIndex":0,"cellParaIndex":0}}]"#
    );
    let end = core
        .resolve_paragraph_by_path(0, para, &caption_path)
        .expect("캡션 문단")
        .char_count as usize
        - 1;
    core.get_cursor_rect_by_path_native(0, para, &caption_json, end)
        .expect("각주 캡션 캐럿");
    core.insert_text_in_cell_by_path(0, para, &caption_path, end, "수달")
        .expect("각주 캡션 입력");

    for bytes in [
        core.export_hwp_native().expect("HWP"),
        core.export_hwpx_native().expect("HWPX"),
    ] {
        let reopened = DocumentCore::from_bytes(&bytes).expect("다시 열기");
        let text = &reopened
            .resolve_paragraph_by_path(0, para, &caption_path)
            .expect("다시 연 각주 캡션")
            .text;
        assert!(text.contains("수달"), "{text}");
    }
}

/// 미주 안 글자처럼 취급 그림도 각주처럼 미주 경로로 알리고 캡션을 단다. 캡션 글줄은 본문 문단이
/// 뿌리인 [(미주, 0, 미주 문단), (그림, 0, 캡션 문단)] 주소로 누르고 그 캐럿 자리를 찾는다.
#[test]
fn endnote_inline_picture_caption_is_addressed_and_hit() {
    let mut core = blank();
    let (para, ctrl) = floating_picture(&mut core, r#"{"treatAsChar":true}"#);
    let picture = core.document.sections[0].paragraphs[para].controls[ctrl].clone();
    let note: serde_json::Value =
        serde_json::from_str(&core.insert_endnote_native(0, para, 0).expect("미주")).expect("JSON");
    let note = note["controlIdx"].as_u64().expect("controlIdx") as usize;
    let inner = {
        let Control::Endnote(endnote) =
            &mut core.document.sections[0].paragraphs[para].controls[note]
        else {
            panic!("미주가 아니다");
        };
        let first = &mut endnote.paragraphs[0];
        let end = first.text.chars().count();
        let at = first.controls.len();
        first.controls.push(picture);
        first.shift_for_inline_control_insert(at, end);
        first.char_count += 8;
        at
    };
    core.document.sections[0].raw_stream = None;
    core.reflow_footnote_paragraph(0, para, note, 0);
    core.mark_section_dirty(0);
    core.paginate_if_needed();
    core.set_cell_picture_properties_by_path_native(
        0,
        para,
        &format!(r#"[{{"controlIndex":{note},"cellIndex":0,"cellParaIndex":0}}]"#),
        inner,
        r#"{"hasCaption":true,"captionDirection":"Bottom","captionVertAlign":"Top","captionWidth":8504,"captionSpacing":850}"#,
    )
    .expect("미주 그림 캡션 넣기");

    let page = (0..core.page_count())
        .find(|&page| {
            core.get_page_control_layout_native(page)
                .is_ok_and(|json| json.contains("\"cellPath\""))
        })
        .expect("미주 그림이 든 쪽");
    let layout: serde_json::Value = serde_json::from_str(
        &core
            .get_page_control_layout_native(page)
            .expect("개체 목록"),
    )
    .expect("JSON");
    let image = layout["controls"]
        .as_array()
        .expect("개체")
        .iter()
        .find(|c| c["type"] == "image" && c["cellPath"].is_array())
        .expect("미주 그림");
    assert_eq!(image["paraIdx"], para, "{image}");
    assert_eq!(image["controlIdx"], inner);
    assert_eq!(image["cellPath"][0]["controlIndex"], note);

    fn caption_line(node: &RenderNode) -> Option<(BoundingBox, CellContext)> {
        if let RenderNodeType::TextLine(line) = &node.node_type {
            if line.caption_owner.is_some() {
                let ctx = node
                    .children
                    .iter()
                    .find_map(|child| match &child.node_type {
                        RenderNodeType::TextRun(run) => run.cell_context.clone(),
                        _ => None,
                    });
                if let Some(ctx) = ctx {
                    return Some((node.bbox, ctx));
                }
            }
        }
        node.children.iter().find_map(caption_line)
    }
    let tree = core.build_page_tree(page).expect("쪽 트리");
    let (line, ctx) = caption_line(&tree.root).expect("미주 캡션 글줄");
    assert_eq!(ctx.parent_para_index, para);
    let steps: Vec<_> = ctx
        .path
        .iter()
        .map(|entry| (entry.control_index, entry.cell_index, entry.cell_para_index))
        .collect();
    assert_eq!(steps, vec![(note, 0, 0), (inner, 0, 0)]);

    let hit: serde_json::Value = serde_json::from_str(
        &core
            .hit_test_native(page, line.x + 2.0, line.y + line.height / 2.0)
            .expect("누르기"),
    )
    .expect("JSON");
    assert_eq!(hit["parentParaIndex"], para, "{hit}");
    assert_eq!(hit["cellPath"][0]["controlIndex"], note);
    assert_eq!(hit["cellPath"][1]["controlIndex"], inner);
    core.get_cursor_rect_by_path_native(
        0,
        para,
        &format!(
            r#"[{{"controlIndex":{note},"cellIndex":0,"cellParaIndex":0}},{{"controlIndex":{inner},"cellIndex":0,"cellParaIndex":0}}]"#
        ),
        0,
    )
    .expect("미주 캡션 캐럿");
}
