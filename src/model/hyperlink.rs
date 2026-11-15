//! HWP/HWPX 하이퍼링크 Command의 공통 주소 인코딩 (#6963).

use crate::error::HwpError;

/// 첫 번째 비이스케이프 세미콜론 앞의 주소를 읽는다.
/// 기존 문서의 mail/file/bookmark도 조회할 수 있도록 읽기에는 scheme 제한을 두지 않는다.
pub fn command_uri(command: &str) -> String {
    let mut out = String::new();
    let mut chars = command.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            ';' => break,
            _ => out.push(c),
        }
    }
    out.trim().to_string()
}

/// 웹 주소를 한컴의 URL 형식 Command로 만든다. 입력을 자동 정규화하지 않는다.
/// 한컴 12.30은 `\ : ; # ?`를 escape한다(HWPX `https\://a.example.org/x\?y=1;1;0;0;`).
/// 이 검사는 scheme/authority/공백 계약이며 DNS 조회나 접속 가능성 검사가 아니다.
pub fn web_command(uri: &str) -> Result<String, HwpError> {
    let invalid = || HwpError::InvalidField("공백 없는 HTTP/HTTPS 절대 주소가 필요합니다".into());
    let (scheme, rest) = uri.split_once("://").ok_or_else(invalid)?;
    if !(scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
        || uri
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
    {
        return Err(invalid());
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.starts_with(':') || authority.contains('@') {
        return Err(invalid());
    }
    let mut command = String::with_capacity(uri.len() + 16);
    for c in uri.chars() {
        if matches!(c, '\\' | ':' | ';' | '#' | '?') {
            command.push('\\');
        }
        command.push(c);
    }
    command.push_str(";1;0;0;");
    checked_length(command)
}

/// 메일 주소를 한컴의 전자 우편 Command로 만든다. 한컴 12.30은 `mailto:a@b;2;0;0`처럼
/// 콜론을 escape하지 않고 종류 2로 끝 세미콜론 없이 저장한다. `\ ;`만 escape한다.
fn mail_command(uri: &str) -> Result<String, HwpError> {
    let mut command = String::with_capacity(uri.len() + 8);
    for c in uri.chars() {
        if matches!(c, '\\' | ';') {
            command.push('\\');
        }
        command.push(c);
    }
    command.push_str(";2;0;0");
    checked_length(command)
}

/// 문서 내부 책갈피 링크의 한컴 Command(`?<이름>;0;0;0;`)를 만든다.
/// 한컴 실측(HWP `hwpctl_Action_Table__v1.1.hwp`, HWPX `TargetType=BOOKMARK`)처럼 `\ : ;`만 escape한다.
pub fn bookmark_command(name: &str) -> Result<String, HwpError> {
    if name.trim().is_empty() || name.chars().any(char::is_control) {
        return Err(HwpError::InvalidField(
            "책갈피 이름이 비어 있거나 제어 문자를 포함합니다".into(),
        ));
    }
    let mut command = String::with_capacity(name.len() + 8);
    command.push('?');
    for c in name.chars() {
        if matches!(c, '\\' | ':' | ';') {
            command.push('\\');
        }
        command.push(c);
    }
    command.push_str(";0;0;0;");
    checked_length(command)
}

/// Command가 지금 문서의 책갈피 링크이면 책갈피 이름을 돌려준다. 개요·표·그림·수식
/// 대상(`?#번호;0;1;0;` 등)과 다른 문서·문서의 처음 링크는 책갈피가 아니다.
pub fn command_bookmark(command: &str) -> Option<String> {
    hwp_link(command)
        .filter(|link| link.path.is_empty() && link.target_type == 0 && !link.anchor.is_empty())
        .map(|link| link.anchor)
}

/// 한컴 Command `주소;종류;대상 종류;여는 방법;`의 첫 칸(이스케이프를 푼 주소)과 숫자 칸들.
/// 한글 문서 링크의 주소는 첫 비이스케이프 `?`에서 파일 경로와 대상(책갈피 이름·`#번호`)으로 나뉜다.
struct CommandParts {
    path: String,
    anchor: Option<String>,
    numbers: Vec<u8>,
}

fn command_parts(command: &str) -> CommandParts {
    let (mut path, mut anchor, mut numbers) = (String::new(), None::<String>, Vec::new());
    let mut chars = command.chars();
    while let Some(c) = chars.next() {
        let c = match c {
            '\\' => match chars.next() {
                Some(next) => next,
                None => break,
            },
            ';' => break,
            '?' if anchor.is_none() => {
                anchor = Some(String::new());
                continue;
            }
            _ => c,
        };
        match &mut anchor {
            Some(anchor) => anchor.push(c),
            None => path.push(c),
        }
    }
    for field in chars.as_str().split(';') {
        numbers.push(field.trim().parse().unwrap_or(0));
    }
    CommandParts {
        path: path.trim().to_string(),
        anchor,
        numbers,
    }
}

/// 한글 문서 링크(종류 0)의 대상. 한컴 12.30 하이퍼링크 창의 한글 문서 탭이 만든다.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct HwpLink {
    /// 다른 한글 문서의 경로. 비면 지금 문서다.
    pub path: String,
    /// 책갈피 이름이나 `#번호`(개요 문단·표·그림·수식의 인스턴스 ID). 비면 문서의 처음이다.
    pub anchor: String,
    /// 0 책갈피(문서의 처음 포함)·1 개요·2 표·3 그림·4 수식.
    pub target_type: u8,
    /// 연결 문서 열기: 0 현재 문서 닫기·1 새 탭·2 새 창.
    pub open: u8,
}

/// 한컴 대상 종류 번호(HWPX `TargetType`).
pub const LINK_TARGET_OUTLINE: u8 = 1;
pub const LINK_TARGET_TABLE: u8 = 2;
pub const LINK_TARGET_FIGURE: u8 = 3;
pub const LINK_TARGET_EQUATION: u8 = 4;

/// Command가 한글 문서 링크면 그 대상을 돌려준다. 종류 칸이 0인 링크다.
pub fn hwp_link(command: &str) -> Option<HwpLink> {
    let parts = command_parts(command);
    if parts.numbers.first().copied() != Some(0) {
        return None;
    }
    Some(HwpLink {
        path: parts.path,
        anchor: parts.anchor.unwrap_or_default(),
        target_type: parts.numbers.get(1).copied().unwrap_or(0),
        open: parts.numbers.get(2).copied().unwrap_or(0),
    })
}

/// Command가 외부 파일 링크(종류 3)면 파일 경로를 돌려준다.
pub fn file_link(command: &str) -> Option<String> {
    let parts = command_parts(command);
    (parts.numbers.first().copied() == Some(3) && !parts.path.is_empty()).then_some(parts.path)
}

fn escaped(text: &str, special: &[char], out: &mut String) -> Result<(), HwpError> {
    if text.chars().any(char::is_control) {
        return Err(HwpError::InvalidField(
            "링크 대상에 제어 문자를 넣을 수 없습니다".into(),
        ));
    }
    for c in text.chars() {
        if c == '\\' || special.contains(&c) {
            out.push('\\');
        }
        out.push(c);
    }
    Ok(())
}

/// 한글 문서 링크의 한컴 Command를 만든다. 한컴 12.30 Mac 저장 꼴(2026-11-16 확인):
/// 문서의 처음 `;0;0;0;`, 개요 `?#번호;0;1;0;`, 표 `?#번호;0;2;0;`, 그림 `?#번호;0;3;0;`,
/// 수식 `?#번호;0;4;0;`, 다른 문서 책갈피 `경로?이름;0;0;0;`, 새 창으로 여는 다른 문서 개요
/// `경로?#번호;0;1;2;`. 경로는 `\ ; ?`를, 대상은 책갈피처럼 `\ : ;`를 이스케이프한다.
pub fn hwp_link_command(link: &HwpLink) -> Result<String, HwpError> {
    if link.target_type > LINK_TARGET_EQUATION || link.open > 2 {
        return Err(HwpError::InvalidField(
            "알 수 없는 링크 대상 종류입니다".into(),
        ));
    }
    if link.target_type != 0 && link.anchor.is_empty() {
        return Err(HwpError::InvalidField("연결할 대상을 골라 주세요".into()));
    }
    let mut command = String::new();
    escaped(&link.path, &[';', '?'], &mut command)?;
    if !link.anchor.is_empty() {
        command.push('?');
        escaped(&link.anchor, &[':', ';'], &mut command)?;
    }
    command.push_str(&format!(";0;{};{};", link.target_type, link.open));
    checked_length(command)
}

/// 외부 파일 링크의 한컴 Command(`경로;3;0;0;`, HWPX `Category=HWPHYPERLINK_TYPE_EX`)를 만든다.
pub fn file_link_command(path: &str) -> Result<String, HwpError> {
    if path.trim().is_empty() {
        return Err(HwpError::InvalidField("연결할 파일을 골라 주세요".into()));
    }
    let mut command = String::new();
    escaped(path.trim(), &[';'], &mut command)?;
    command.push_str(";3;0;0;");
    checked_length(command)
}

fn checked_length(command: String) -> Result<String, HwpError> {
    // HWP5 CTRL_HEADER의 command 길이는 u16 UTF-16 code unit 수다.
    if command.encode_utf16().count() > u16::MAX as usize {
        return Err(HwpError::InvalidField(
            "하이퍼링크 주소가 너무 깁니다".into(),
        ));
    }
    Ok(command)
}

/// Destination semantics shared by editing, output and host click routing.
/// Classification never opens a file, network connection or application.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkDestinationKind {
    Web,
    Mail,
    DocumentAnchor,
    RelativeFile,
    LocalFile,
    RegisteredApplication,
}

pub fn classify_destination(uri: &str) -> Option<LinkDestinationKind> {
    use LinkDestinationKind::*;
    if uri.is_empty() || uri.trim() != uri || uri.chars().any(char::is_control) {
        return None;
    }
    if let Some(anchor) = uri.strip_prefix('#') {
        return (!anchor.is_empty()).then_some(DocumentAnchor);
    }
    if uri.starts_with('/')
        || uri.starts_with("\\\\")
        || (uri.as_bytes().get(1) == Some(&b':')
            && uri.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
            && uri
                .as_bytes()
                .get(2)
                .is_some_and(|b| matches!(b, b'/' | b'\\')))
    {
        return Some(LocalFile);
    }
    if let Some((scheme, rest)) = uri.split_once(':') {
        if rest.is_empty()
            || !scheme
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
            || !scheme
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"+-.".contains(&b))
        {
            return None;
        }
        return match scheme.to_ascii_lowercase().as_str() {
            "http" | "https" => web_command(uri).ok().map(|_| Web),
            "mailto" => (!rest.trim().is_empty()).then_some(Mail),
            "file" => Some(LocalFile),
            "javascript" | "vbscript" | "data" => None,
            _ => Some(RegisteredApplication),
        };
    }
    Some(RelativeFile)
}

/// Preserve the selected destination verbatim through HWP/HWPX command escaping.
pub fn destination_command(uri: &str) -> Result<String, HwpError> {
    destination_command_with_mail_form(uri, true)
}

/// `mail_form` 이 거짓이면 `mailto:` 주소도 웹 주소 꼴(`mailto\:a@b;1;0;0;`)로 적는다.
/// 한컴 12.30은 사용자가 `mailto:` 를 직접 친 주소를 입력어 자동 실행으로 걸 때 이 꼴로
/// 저장한다(전자 우편 종류가 아니라 URL 종류). `a@b.c` 꼴을 걸 때는 전자 우편 꼴이다.
pub fn destination_command_with_mail_form(uri: &str, mail_form: bool) -> Result<String, HwpError> {
    match classify_destination(uri)
        .ok_or_else(|| HwpError::InvalidField("유효한 링크 목적지가 필요합니다".into()))?
    {
        LinkDestinationKind::Web => return web_command(uri),
        LinkDestinationKind::Mail if mail_form => return mail_command(uri),
        _ => {}
    }
    let mut command = String::with_capacity(uri.len() + 16);
    for c in uri.chars() {
        if matches!(c, '\\' | ':' | ';' | '#') {
            command.push('\\');
        }
        command.push(c);
    }
    command.push_str(";1;0;0;");
    if command.encode_utf16().count() > u16::MAX as usize {
        return Err(HwpError::InvalidField(
            "하이퍼링크 주소가 너무 깁니다".into(),
        ));
    }
    Ok(command)
}
