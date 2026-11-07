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

fn handle_request(req_line: &str) -> Option<(&'static [u8], &'static str)> {
    let path = req_line.split_whitespace().nth(1)?;
    let clean_path = path.split('?').next().unwrap_or(path);

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
            let _ = std::process::Command::new("open").arg(&url).spawn();
        }
        #[cfg(target_os = "windows")]
        {
            let _ = std::process::Command::new("cmd")
                .args(["/C", "start", &url])
                .spawn();
        }
        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        {
            let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
        }
    }

    for mut stream in listener.incoming().flatten() {
        use std::io::{BufRead, BufReader, Write};
        let mut reader = BufReader::new(&stream);
        let mut req_line = String::new();
        if reader.read_line(&mut req_line).is_ok() {
            if let Some((content, mime)) = handle_request(&req_line) {
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    mime,
                    content.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(content);
            } else {
                let not_found = b"HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\nConnection: close\r\n\r\nNot Found";
                let _ = stream.write_all(not_found);
            }
            let _ = stream.flush();
        }
    }

    Ok(())
}
