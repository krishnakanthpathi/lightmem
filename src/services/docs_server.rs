use anyhow::Result;
use std::net::TcpListener;

const INDEX_HTML: &str = include_str!("../../static/index.html");
const DOCS_HTML: &str = include_str!("../../static/docs.html");
const LANDING_CSS: &str = include_str!("../../static/landing.css");
const DOCS_CSS: &str = include_str!("../../static/docs.css");
const THEME_CSS: &str = include_str!("../../static/theme.css");
const DOCS_JS: &str = include_str!("../../static/docs.js");
const SITE_JS: &str = include_str!("../../static/site.js");
const HEX_VAULT_JS: &str = include_str!("../../static/hex-vault.js");
const LOGO_PNG: &[u8] = include_bytes!("../../static/logo.png");
const FAVICON_32: &[u8] = include_bytes!("../../static/favicon-32.png");
const APPLE_TOUCH_ICON: &[u8] = include_bytes!("../../static/apple-touch-icon.png");
const OG_PNG: &[u8] = include_bytes!("../../static/og.png");

fn route_path(clean_path: &str) -> Option<(&'static [u8], &'static str)> {
    match clean_path {
        "/" | "/index.html" => Some((INDEX_HTML.as_bytes(), "text/html; charset=utf-8")),
        "/docs" | "/docs.html" => Some((DOCS_HTML.as_bytes(), "text/html; charset=utf-8")),
        "/landing.css" => Some((LANDING_CSS.as_bytes(), "text/css; charset=utf-8")),
        "/docs.css" => Some((DOCS_CSS.as_bytes(), "text/css; charset=utf-8")),
        "/theme.css" => Some((THEME_CSS.as_bytes(), "text/css; charset=utf-8")),
        "/docs.js" => Some((DOCS_JS.as_bytes(), "application/javascript; charset=utf-8")),
        "/site.js" => Some((SITE_JS.as_bytes(), "application/javascript; charset=utf-8")),
        "/hex-vault.js" => Some((
            HEX_VAULT_JS.as_bytes(),
            "application/javascript; charset=utf-8",
        )),
        "/logo.png" => Some((LOGO_PNG, "image/png")),
        "/favicon-32.png" | "/favicon.ico" => Some((FAVICON_32, "image/png")),
        "/apple-touch-icon.png" => Some((APPLE_TOUCH_ICON, "image/png")),
        "/og.png" => Some((OG_PNG, "image/png")),
        _ => None,
    }
}

fn handle_request(req_line: &str) -> Option<(&'static [u8], &'static str)> {
    let mut parts = req_line.split_whitespace();
    let method = parts.next()?;
    let path = parts.next()?;
    if !matches!(method, "GET" | "HEAD") {
        return None;
    }
    let clean_path = path.split('?').next().unwrap_or(path);
    route_path(clean_path)
}

fn serve_connection(mut stream: std::net::TcpStream) {
    use std::io::{BufRead, BufReader, Write};
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(10)));

    let mut req_line = String::new();
    {
        let mut reader = BufReader::new(&stream);
        if reader.read_line(&mut req_line).unwrap_or(0) == 0 {
            return;
        }
        // Drain remaining HTTP headers up to the blank line so closing the socket
        // does not leave unread receive-buffer bytes that trigger TCP RST on macOS/BSD.
        let mut header_line = String::new();
        let mut total_header_bytes = req_line.len();
        loop {
            header_line.clear();
            match reader.read_line(&mut header_line) {
                Ok(0) => break,
                Ok(n) => {
                    total_header_bytes = total_header_bytes.saturating_add(n);
                    if header_line == "\r\n" || header_line == "\n" || total_header_bytes >= 131_072
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    }

    let method = req_line.split_whitespace().next().unwrap_or("");
    let is_head = method == "HEAD";
    if !method.is_empty() && !matches!(method, "GET" | "HEAD") {
        let resp = b"HTTP/1.1 405 Method Not Allowed\r\nAllow: GET, HEAD\r\nContent-Length: 18\r\nConnection: close\r\n\r\nMethod Not Allowed";
        let _ = stream.write_all(resp);
    } else if let Some((content, mime)) = handle_request(&req_line) {
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
            mime,
            content.len()
        );
        let _ = stream.write_all(header.as_bytes());
        if !is_head {
            let _ = stream.write_all(content);
        }
    } else {
        let not_found_hdr =
            b"HTTP/1.1 404 Not Found\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: 9\r\nConnection: close\r\n\r\n";
        let _ = stream.write_all(not_found_hdr);
        if !is_head {
            let _ = stream.write_all(b"Not Found");
        }
    }
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Write);
}

/// Options for launching the embedded documentation and landing page server
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DocsOptions {
    pub port: Option<u16>,
    pub open_landing: bool,
    pub no_browser: bool,
}

impl DocsOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    pub fn landing(mut self, open_landing: bool) -> Self {
        self.open_landing = open_landing;
        self
    }

    pub fn no_browser(mut self, no_browser: bool) -> Self {
        self.no_browser = no_browser;
        self
    }
}

/// Serve embedded documentation and landing page via local HTTP server
pub fn serve_docs(options: DocsOptions) -> Result<()> {
    let mut listener = None;
    let mut port = options.port.unwrap_or(8080);

    if let Some(p) = options.port {
        listener = Some(TcpListener::bind(format!("127.0.0.1:{}", p))?);
    } else {
        for p in 8080..8100 {
            if let Ok(l) = TcpListener::bind(format!("127.0.0.1:{}", p)) {
                listener = Some(l);
                port = p;
                break;
            }
        }
    }

    let listener = listener.ok_or_else(|| {
        anyhow::anyhow!("Could not bind documentation server port (tried 8080..8100)")
    })?;

    let start_path = if options.open_landing { "" } else { "docs" };
    let url = format!("http://localhost:{}/{}", port, start_path);

    println!();
    println!("  \x1b[1;38;2;220;38;38m❖\x1b[0m  \x1b[1;38;2;248;250;252mL I G H T M E M\x1b[0m  \x1b[38;2;113;113;122mLocal Documentation Server\x1b[0m");
    println!("  \x1b[38;2;220;38;38m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m");
    println!(
        "  \x1b[1;38;2;16;185;129m▸ Documentation:\x1b[0m  \x1b[38;2;96;165;250mhttp://localhost:{}/docs\x1b[0m",
        port
    );
    println!(
        "  \x1b[1;38;2;16;185;129m▸ Landing Page:\x1b[0m   \x1b[38;2;96;165;250mhttp://localhost:{}/\x1b[0m",
        port
    );
    println!("  \x1b[38;2;148;163;184mServing 100% offline from standalone binary. Press Ctrl+C to stop.\x1b[0m\n");

    if !options.no_browser {
        #[cfg(target_os = "macos")]
        {
            if let Err(e) = std::process::Command::new("open").arg(&url).spawn() {
                eprintln!("Warning: Failed to open {} in browser: {}", url, e);
            }
        }
        #[cfg(target_os = "windows")]
        {
            if let Err(e) = std::process::Command::new("cmd")
                .args(["/C", "start", &url])
                .spawn()
            {
                eprintln!("Warning: Failed to open {} in browser: {}", url, e);
            }
        }
        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        {
            if let Err(e) = std::process::Command::new("xdg-open").arg(&url).spawn() {
                eprintln!("Warning: Failed to open {} in browser: {}", url, e);
            }
        }
    }

    for stream in listener.incoming().flatten() {
        std::thread::spawn(move || serve_connection(stream));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;

    #[test]
    fn test_docs_options_builder_and_routes() {
        let opts = DocsOptions::new()
            .with_port(8095)
            .landing(true)
            .no_browser(true);
        assert_eq!(opts.port, Some(8095));
        assert!(opts.open_landing);
        assert!(opts.no_browser);

        let (index_bytes, index_mime) = handle_request("GET / HTTP/1.1").unwrap();
        assert_eq!(index_mime, "text/html; charset=utf-8");
        assert!(!index_bytes.is_empty());

        let (docs_bytes, docs_mime) = handle_request("GET /docs?q=test HTTP/1.1").unwrap();
        assert_eq!(docs_mime, "text/html; charset=utf-8");
        assert!(!docs_bytes.is_empty());

        let (css_bytes, css_mime) = handle_request("GET /docs.css HTTP/1.1").unwrap();
        assert_eq!(css_mime, "text/css; charset=utf-8");
        assert!(!css_bytes.is_empty());

        let (png_bytes, png_mime) = handle_request("GET /favicon.ico HTTP/1.1").unwrap();
        assert_eq!(png_mime, "image/png");
        assert!(!png_bytes.is_empty());

        assert!(handle_request("GET /nonexistent.txt HTTP/1.1").is_none());
        assert!(handle_request("POST /docs HTTP/1.1").is_none());
    }

    #[test]
    fn test_docs_tcp_large_headers_head_and_404_without_rst() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let srv = std::thread::spawn(move || {
            for stream in listener.incoming().take(3).flatten() {
                serve_connection(stream);
            }
        });

        // 1. GET /docs with >24KB headers (would previously trigger TCP RST / ConnectionResetError)
        let mut s1 = TcpStream::connect(addr).unwrap();
        let big_hdr = "Z".repeat(24_000);
        write!(
            s1,
            "GET /docs HTTP/1.1\r\nHost: {}\r\nX-Big: {}\r\nConnection: close\r\n\r\n",
            addr, big_hdr
        )
        .unwrap();
        s1.flush().unwrap();
        let mut resp1 = Vec::new();
        s1.read_to_end(&mut resp1).unwrap();
        let resp1_str = String::from_utf8_lossy(&resp1);
        assert!(resp1_str.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(resp1_str.contains(&format!("Content-Length: {}\r\n", DOCS_HTML.len())));
        assert!(resp1.ends_with(DOCS_HTML.as_bytes()));

        // 2. HEAD / returns headers with Content-Length and zero body
        let mut s2 = TcpStream::connect(addr).unwrap();
        write!(s2, "HEAD / HTTP/1.1\r\nHost: {}\r\n\r\n", addr).unwrap();
        s2.flush().unwrap();
        let mut resp2 = Vec::new();
        s2.read_to_end(&mut resp2).unwrap();
        let resp2_str = String::from_utf8_lossy(&resp2);
        assert!(resp2_str.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(resp2_str.ends_with("\r\n\r\n"));

        // 3. POST / returns 405 Method Not Allowed
        let mut s3 = TcpStream::connect(addr).unwrap();
        write!(s3, "POST /docs HTTP/1.1\r\nHost: {}\r\n\r\n", addr).unwrap();
        s3.flush().unwrap();
        let mut resp3 = String::new();
        s3.read_to_string(&mut resp3).unwrap();
        assert!(resp3.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"));

        srv.join().unwrap();
    }
}
