//! 차트 제목(`c:chart` 바로 아래 `c:title`) 읽기·쓰기.
//!
//! 한컴 12.30(Mac) 차트 데이터 편집 창의 "차트 제목" 칸과 같은 값이다. 한컴은 제목을 고치면
//! `c:title` 을 `c:tx/c:rich`(14pt 함초롬돋움 글자 하나)로 다시 쓰고 `c:txPr` 를 뺀다. 칸을 고치지
//! 않으면 쓰지 않는다(빈 `c:title` + `autoTitleDeleted=0` 은 자동 제목으로 그린다).

use crate::parser::parse_chart_xml;

/// 제목을 쓰지 못한 까닭.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TitleError {
    /// XML 이 UTF-8 이 아니거나 `c:chart`·`c:plotArea` 를 찾지 못했다.
    NoChart,
    /// XML 을 깨뜨리는 문자(`<`, `>`, `&`, 제어 문자)가 들어 있다.
    UnsafeText,
}

impl std::fmt::Display for TitleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoChart => write!(f, "차트 XML 에서 c:chart·c:plotArea 를 찾지 못했다"),
            Self::UnsafeText => write!(f, "차트 제목에 XML 특수문자가 있다"),
        }
    }
}

impl std::error::Error for TitleError {}

/// `c:chart` 여는 태그 뒤부터 `c:plotArea` 앞까지(차트 제목 자리)와 접두사.
struct Head<'a> {
    text: &'a str,
    /// 차트 이름공간 접두사(`c:`).
    c: String,
    /// DrawingML 접두사. 문서가 선언하지 않았으면 `None`.
    a: Option<String>,
    body: usize,
    plot: usize,
}

impl<'a> Head<'a> {
    fn locate(xml: &'a [u8]) -> Option<Self> {
        let text = std::str::from_utf8(xml).ok()?;
        // 접두사는 문서의 xmlns 선언을 따른다(한컴·Excel 은 `c:`, `a:`).
        let prefix = |uri: &str| {
            let end = text.find(&format!("=\"{uri}\""))?;
            let start = text[..end].rfind("xmlns:")? + 6;
            let name = &text[start..end];
            (!name.contains([' ', '"', '='])).then(|| format!("{name}:"))
        };
        let c = prefix("http://schemas.openxmlformats.org/drawingml/2006/chart")
            .unwrap_or_else(|| "c:".to_string());
        let a = prefix("http://schemas.openxmlformats.org/drawingml/2006/main");
        let open = [format!("<{c}chart>"), format!("<{c}chart ")]
            .iter()
            .filter_map(|tag| text.find(tag.as_str()))
            .min()?;
        let body = open + text[open..].find('>')? + 1;
        let plot = body + text[body..].find(&format!("<{c}plotArea"))?;
        Some(Self {
            text,
            c,
            a,
            body,
            plot,
        })
    }

    fn head(&self) -> &'a str {
        &self.text[self.body..self.plot]
    }

    /// `within` 안 첫 `c:name` 요소 구간(빈 요소 `<x/>` 또는 `<x ...>...</x>`).
    /// 축 제목은 plotArea 안이라 차트 제목 자리에 없다.
    fn element(&self, within: &str, name: &str) -> Option<std::ops::Range<usize>> {
        let tag = format!("<{}{name}", self.c);
        let (start, _) = within
            .match_indices(tag.as_str())
            .find(|&(i, _)| within[i + tag.len()..].starts_with(['>', ' ', '/']))?;
        let gt = start + within[start..].find('>')?;
        if within[..gt].ends_with('/') {
            return Some(start..gt + 1);
        }
        let close = format!("</{}{name}>", self.c);
        let end = gt + within[gt..].find(&close)? + close.len();
        Some(start..end)
    }
}

/// 화면에 보이는 차트 제목. 명시 텍스트, 없으면 렌더러와 같은 자동 제목(계열 하나면 그 이름,
/// 아니면 "차트 제목")이다. 제목을 숨긴 차트(`c:title` 없음·`autoTitleDeleted=1`)는 `None`.
pub fn chart_title(xml: &[u8]) -> Option<String> {
    let head = Head::locate(xml)?;
    let region = head.head();
    let title = head.element(region, "title").map(|span| &region[span]);
    if let Some(title) = title {
        let a = head.a.as_deref().unwrap_or("a:");
        let open = format!("<{a}t");
        let close = format!("</{a}t>");
        let mut text = String::new();
        for (i, _) in title.match_indices(open.as_str()) {
            let rest = &title[i + open.len()..];
            if !rest.starts_with(['>', ' ']) {
                continue;
            }
            let Some(gt) = rest.find('>') else { continue };
            let Some(end) = rest.find(&close) else {
                continue;
            };
            if gt < end {
                text.push_str(&rest[gt + 1..end]);
            }
        }
        if !text.is_empty() {
            return Some(
                text.replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&quot;", "\"")
                    .replace("&apos;", "'")
                    .replace("&amp;", "&"),
            );
        }
    }
    let deleted = head
        .element(region, "autoTitleDeleted")
        .is_some_and(|span| {
            let element = &region[span];
            element.contains("\"1\"") || element.contains("\"true\"")
        });
    if title.is_none() || deleted {
        return None;
    }
    let chart = parse_chart_xml(xml)?;
    Some(match &chart.series[..] {
        [only] if !only.name.is_empty() => only.name.clone(),
        _ => "차트 제목".to_string(),
    })
}

/// 차트 제목을 바꾼 XML. 빈 제목은 제목을 지운다(`c:title` 을 빼고 `autoTitleDeleted=1`).
pub fn set_chart_title(xml: &[u8], title: &str) -> Result<Vec<u8>, TitleError> {
    if title
        .chars()
        .any(|c| matches!(c, '<' | '>' | '&') || c.is_control())
    {
        return Err(TitleError::UnsafeText);
    }
    let head = Head::locate(xml).ok_or(TitleError::NoChart)?;
    let (text, c, region) = (head.text, head.c.as_str(), head.head());
    let a = head.a.as_deref().unwrap_or("a:");
    let title_span = head.element(region, "title");
    let title_xml = if title.is_empty() {
        String::new()
    } else {
        // 기존 제목의 배치·겹침·도형 서식은 두고 글(tx)만 바꾼다. 글자 서식은 한컴처럼 글에 싣는다.
        let old = title_span.clone().map_or("", |span| &region[span]);
        let mut keep: String = ["layout", "overlay", "spPr", "extLst"]
            .iter()
            .filter_map(|name| head.element(old, name).map(|span| &old[span]))
            .collect();
        if keep.is_empty() {
            keep = format!("<{c}layout/><{c}overlay val=\"0\"/>");
        }
        let xmlns = if head.a.is_some() {
            ""
        } else {
            " xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\""
        };
        let run = format!(
            "sz=\"1400\" b=\"0\" i=\"0\" u=\"none\"><{a}solidFill><{a}sysClr val=\"windowText\" \
             lastClr=\"000000\"/></{a}solidFill><{a}latin typeface=\"함초롬돋움\"/>\
             <{a}ea typeface=\"함초롬돋움\"/><{a}cs typeface=\"함초롬돋움\"/>\
             <{a}sym typeface=\"함초롬돋움\"/>"
        );
        format!(
            "<{c}title><{c}tx><{c}rich{xmlns}><{a}bodyPr rot=\"0\" vert=\"horz\" wrap=\"none\" \
             lIns=\"0\" tIns=\"0\" rIns=\"0\" bIns=\"0\" anchor=\"ctr\" anchorCtr=\"1\"/><{a}p>\
             <{a}pPr algn=\"l\"><{a}defRPr {run}</{a}defRPr></{a}pPr><{a}r><{a}rPr {run}</{a}rPr>\
             <{a}t>{title}</{a}t></{a}r><{a}endParaRPr/></{a}p></{c}rich></{c}tx>{keep}</{c}title>"
        )
    };

    // `autoTitleDeleted`(c:title 바로 뒤)는 제목을 지우면 1, 쓰면 0 이다.
    let deleted = format!(
        "<{c}autoTitleDeleted val=\"{}\"/>",
        u8::from(title.is_empty())
    );
    let title_span = title_span.unwrap_or(0..0);
    let auto = head
        .element(region, "autoTitleDeleted")
        .filter(|span| span.start >= title_span.end);
    let mut out = String::with_capacity(text.len() + title_xml.len() + deleted.len());
    out.push_str(&text[..head.body]);
    out.push_str(&region[..title_span.start]);
    out.push_str(&title_xml);
    match auto {
        Some(auto) => {
            out.push_str(&region[title_span.end..auto.start]);
            out.push_str(&deleted);
            out.push_str(&region[auto.end..]);
        }
        None => {
            out.push_str(&deleted);
            out.push_str(&region[title_span.end..]);
        }
    }
    out.push_str(&text[head.plot..]);
    Ok(out.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HANCOM_DEFAULT: &str = concat!(
        r#"<c:chartSpace xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" "#,
        r#"xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart>"#,
        r#"<c:title><c:layout/><c:overlay val="0"/><c:txPr><a:bodyPr/><a:p><a:pPr algn="l">"#,
        r#"<a:defRPr b="0"/></a:pPr><a:endParaRPr/></a:p></c:txPr></c:title>"#,
        r#"<c:autoTitleDeleted val="0"/><c:plotArea><c:layout/><c:barChart><c:barDir val="bar"/>"#,
        r#"<c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:v>계열 1</c:v></c:tx>"#,
        r#"<c:val><c:numRef><c:numCache><c:ptCount val="1"/><c:pt idx="0"><c:v>1</c:v></c:pt>"#,
        r#"</c:numCache></c:numRef></c:val></c:ser>"#,
        r#"<c:ser><c:idx val="1"/><c:order val="1"/><c:tx><c:v>계열 2</c:v></c:tx>"#,
        r#"<c:val><c:numRef><c:numCache><c:ptCount val="1"/><c:pt idx="0"><c:v>2</c:v></c:pt>"#,
        r#"</c:numCache></c:numRef></c:val></c:ser>"#,
        r#"</c:barChart><c:valAx><c:title><c:tx><c:rich><a:p><a:r><a:t>축</a:t></a:r></a:p>"#,
        r#"</c:rich></c:tx></c:title></c:valAx></c:plotArea></c:chart></c:chartSpace>"#,
    );

    #[test]
    fn hancom_default_chart_shows_auto_title() {
        assert_eq!(
            chart_title(HANCOM_DEFAULT.as_bytes()).as_deref(),
            Some("차트 제목")
        );
    }

    #[test]
    fn set_title_writes_hancom_rich_text_and_keeps_axis_title() {
        let out = set_chart_title(HANCOM_DEFAULT.as_bytes(), "성적표").unwrap();
        let text = String::from_utf8(out.clone()).unwrap();
        assert_eq!(chart_title(&out).as_deref(), Some("성적표"));
        assert!(text.contains(r#"<a:t>성적표</a:t>"#), "{text}");
        assert!(text.contains(r#"<c:autoTitleDeleted val="0"/>"#), "{text}");
        // 한컴처럼 글자 서식은 c:tx 에 싣고 c:txPr 를 뺀다. 배치·겹침은 남긴다.
        assert!(
            !text[..text.find("<c:plotArea").unwrap()].contains("txPr"),
            "{text}"
        );
        assert!(
            text.contains(r#"</c:tx><c:layout/><c:overlay val="0"/></c:title>"#),
            "{text}"
        );
        assert!(text.contains("<a:t>축</a:t>"), "{text}");
        // 다시 쓰면 바뀐 글만 남는다.
        let again = set_chart_title(&out, "매출").unwrap();
        assert_eq!(chart_title(&again).as_deref(), Some("매출"));
        assert_eq!(
            String::from_utf8(again)
                .unwrap()
                .matches("<c:title>")
                .count(),
            2
        );
    }

    #[test]
    fn empty_title_hides_title_and_new_title_shows_it_again() {
        let hidden = set_chart_title(HANCOM_DEFAULT.as_bytes(), "").unwrap();
        let text = String::from_utf8(hidden.clone()).unwrap();
        assert_eq!(chart_title(&hidden), None);
        assert!(
            text.contains(r#"<c:chart><c:autoTitleDeleted val="1"/><c:plotArea>"#),
            "{text}"
        );
        let shown = set_chart_title(&hidden, "새 제목").unwrap();
        let text = String::from_utf8(shown.clone()).unwrap();
        assert_eq!(chart_title(&shown).as_deref(), Some("새 제목"));
        assert!(
            text.contains(r#"</c:title><c:autoTitleDeleted val="0"/><c:plotArea>"#),
            "{text}"
        );
    }

    #[test]
    fn unsafe_title_is_refused() {
        assert_eq!(
            set_chart_title(HANCOM_DEFAULT.as_bytes(), "a<b"),
            Err(TitleError::UnsafeText)
        );
    }
}
