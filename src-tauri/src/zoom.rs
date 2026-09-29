//! Zoom デスクトップアプリへの自動参加と会議ウィンドウの検知

use log::info;

/// 会議 URL から取り出した参加情報
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoomMeeting {
    /// 会議 ID（数字のみ）
    pub confno: String,
    /// パスコード（URL に埋め込まれた暗号化済みの pwd）
    pub pwd: Option<String>,
    /// 会議をホストしているドメイン（例: us02web.zoom.us, company.zoom.us）
    pub domain: String,
}

/// Zoom の会議 URL を解析する。
///
/// 対応形式:
/// - `https://*.zoom.us/j/<id>?pwd=...`
/// - `https://*.zoom.us/w/<id>?...` / `https://*.zoom.us/s/<id>`（ウェビナー等）
/// - `https://*.zoom.us/wc/join/<id>`
/// - `zoommtg://zoom.us/join?confno=<id>&pwd=...`
pub fn parse_meeting_url(url: &str) -> Option<ZoomMeeting> {
    let url = url.trim();
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if !matches!(scheme.as_str(), "https" | "http" | "zoommtg" | "zoomus") {
        return None;
    }

    let (host_and_path, query) = match rest.split_once('?') {
        Some((hp, q)) => (hp, Some(q.split('#').next().unwrap_or(q))),
        None => (rest.split('#').next().unwrap_or(rest), None),
    };
    let (host, path) = match host_and_path.split_once('/') {
        Some((h, p)) => (h.to_ascii_lowercase(), p),
        None => (host_and_path.to_ascii_lowercase(), ""),
    };
    // ポート番号を除去
    let host = host.split(':').next().unwrap_or(&host).to_string();

    let is_zoom_host = host == "zoom.us"
        || host.ends_with(".zoom.us")
        || host == "zoomgov.com"
        || host.ends_with(".zoomgov.com");
    if !is_zoom_host {
        return None;
    }

    let param = |name: &str| -> Option<String> {
        query?.split('&').find_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            (k == name && !v.is_empty()).then(|| {
                urlencoding::decode(v)
                    .map(|s| s.into_owned())
                    .unwrap_or_else(|_| v.to_string())
            })
        })
    };

    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let id_from_path = match segments.as_slice() {
        ["j" | "w" | "s", id, ..] => Some(*id),
        ["wc", "join", id, ..] => Some(*id),
        ["wc", id, "join", ..] => Some(*id),
        _ => None,
    };

    let confno = id_from_path
        .map(|s| s.to_string())
        .or_else(|| param("confno"))?;
    let confno: String = confno.chars().filter(|c| c.is_ascii_digit()).collect();
    if confno.len() < 9 {
        return None;
    }

    let domain = if scheme.starts_with("zoom") {
        "zoom.us".to_string()
    } else {
        host
    };

    Some(ZoomMeeting {
        confno,
        pwd: param("pwd"),
        domain,
    })
}

/// Zoom アプリを起動するためのプロトコル URL を組み立てる
pub fn build_join_url(meeting: &ZoomMeeting, display_name: &str) -> String {
    let mut url = format!(
        "zoommtg://{}/join?action=join&confno={}",
        meeting.domain, meeting.confno
    );
    if let Some(pwd) = &meeting.pwd {
        url.push_str("&pwd=");
        url.push_str(&urlencoding::encode(pwd));
    }
    let name = display_name.trim();
    if !name.is_empty() {
        url.push_str("&uname=");
        url.push_str(&urlencoding::encode(name));
    }
    url
}

/// Zoom デスクトップアプリで会議に参加する（ブラウザを経由しない）
pub fn join(meeting: &ZoomMeeting, display_name: &str) -> Result<(), String> {
    let url = build_join_url(meeting, display_name);
    info!("Launching Zoom: confno={}", meeting.confno);
    tauri_plugin_opener::open_url(&url, None::<&str>).map_err(|e| {
        format!(
            "Zoom アプリを起動できませんでした（インストールされていますか？）: {}",
            e
        )
    })
}

/// Zoom の会議ウィンドウを探し、HWND を返す
#[cfg(windows)]
pub fn find_meeting_window() -> Option<isize> {
    win::find_meeting_window()
}

#[cfg(not(windows))]
pub fn find_meeting_window() -> Option<isize> {
    None
}

#[cfg(windows)]
mod win {
    use windows::core::{BOOL, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
        IsWindowVisible,
    };

    /// Zoom の会議ウィンドウのクラス名（バージョンにより異なる）
    const MEETING_CLASSES: &[&str] = &["ConfMultiTabContentWndClass", "ZPContentViewWndClass"];
    /// クラス名で判定できない場合のタイトル（各言語）
    const MEETING_TITLES: &[&str] = &["Zoom Meeting", "Zoom ミーティング", "Zoomミーティング", "Zoom Webinar", "Zoom ウェビナー"];

    struct Search {
        found: Option<isize>,
        fallback: Option<isize>,
    }

    fn process_name(pid: u32) -> Option<String> {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let result = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                PWSTR(buf.as_mut_ptr()),
                &mut len,
            );
            let _ = CloseHandle(handle);
            result.ok()?;
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            path.rsplit(['\\', '/']).next().map(|s| s.to_ascii_lowercase())
        }
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = &mut *(lparam.0 as *mut Search);

        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }

        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid as *mut u32));
        if process_name(pid).as_deref() != Some("zoom.exe") {
            return BOOL(1);
        }

        let mut class_buf = [0u16; 256];
        let class_len = GetClassNameW(hwnd, &mut class_buf);
        let class = String::from_utf16_lossy(&class_buf[..class_len.max(0) as usize]);

        let mut title_buf = [0u16; 512];
        let title_len = GetWindowTextW(hwnd, &mut title_buf);
        let title = String::from_utf16_lossy(&title_buf[..title_len.max(0) as usize]);

        let minimized = IsIconic(hwnd).as_bool();
        if MEETING_CLASSES.contains(&class.as_str()) {
            if !minimized {
                search.found = Some(hwnd.0 as isize);
                return BOOL(0); // 見つかったので列挙を終了
            }
            search.fallback.get_or_insert(hwnd.0 as isize);
        } else if MEETING_TITLES.iter().any(|t| title.starts_with(t)) {
            search.fallback.get_or_insert(hwnd.0 as isize);
        }
        BOOL(1)
    }

    pub fn find_meeting_window() -> Option<isize> {
        let mut search = Search {
            found: None,
            fallback: None,
        };
        unsafe {
            // コールバックが FALSE を返して列挙を中断した場合もエラー扱いになるので結果は無視する
            let _ = EnumWindows(Some(enum_proc), LPARAM(&mut search as *mut Search as isize));
        }
        search.found.or(search.fallback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_join_url() {
        let m = parse_meeting_url("https://us02web.zoom.us/j/81234567890?pwd=AbC123.1").unwrap();
        assert_eq!(m.confno, "81234567890");
        assert_eq!(m.pwd.as_deref(), Some("AbC123.1"));
        assert_eq!(m.domain, "us02web.zoom.us");
    }

    #[test]
    fn parses_url_without_password_and_with_fragment() {
        let m = parse_meeting_url("https://zoom.us/j/1234567890#success").unwrap();
        assert_eq!(m.confno, "1234567890");
        assert_eq!(m.pwd, None);
    }

    #[test]
    fn parses_vanity_and_webinar_urls() {
        let m = parse_meeting_url("https://company.zoom.us/w/99988877766?tk=x&pwd=p").unwrap();
        assert_eq!(m.confno, "99988877766");
        assert_eq!(m.pwd.as_deref(), Some("p"));
        assert_eq!(m.domain, "company.zoom.us");

        let m = parse_meeting_url("https://zoom.us/wc/join/123456789").unwrap();
        assert_eq!(m.confno, "123456789");
    }

    #[test]
    fn parses_protocol_url() {
        let m = parse_meeting_url("zoommtg://zoom.us/join?action=join&confno=123456789&pwd=abc")
            .unwrap();
        assert_eq!(m.confno, "123456789");
        assert_eq!(m.pwd.as_deref(), Some("abc"));
    }

    #[test]
    fn rejects_non_zoom_urls() {
        assert!(parse_meeting_url("https://meet.google.com/abc-defg-hij").is_none());
        assert!(parse_meeting_url("https://zoom.us.evil.com/j/123456789").is_none());
        assert!(parse_meeting_url("https://zoom.us/profile").is_none());
        assert!(parse_meeting_url("not a url").is_none());
    }

    #[test]
    fn builds_protocol_url_with_encoded_name() {
        let m = ZoomMeeting {
            confno: "123456789".into(),
            pwd: Some("a b".into()),
            domain: "zoom.us".into(),
        };
        assert_eq!(
            build_join_url(&m, "録画 Bot"),
            "zoommtg://zoom.us/join?action=join&confno=123456789&pwd=a%20b&uname=%E9%8C%B2%E7%94%BB%20Bot"
        );
    }
}
