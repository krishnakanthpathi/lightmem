use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufReader, BufWriter, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveFolders {
    pub root_id: String,
    pub blobs_id: String,
    pub snapshots_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GDriveTokenFile {
    pub access_token: String,
    pub expires_in: Option<i64>,
    pub refresh_token: Option<String>,
    pub token_type: Option<String>,
    pub user_email: Option<String>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveAuthStatus {
    pub connected: bool,
    pub user_email: Option<String>,
    pub scopes: Vec<String>,
    pub credentials_present: bool,
    pub token_present: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub folders: Option<DriveFolders>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveLogoutReport {
    pub user_email: Option<String>,
    pub remote_revoked: bool,
    pub token_removed: bool,
    pub folders_cache_removed: bool,
    pub credentials_removed: bool,
}

pub struct GoogleDriveClient {
    client_id: String,
    client_secret: String,
    refresh_token: String,
    pub user_email: String,
    token_cache: Mutex<Option<(String, DateTime<Utc>)>>,
}

fn url_encode(input: &str) -> String {
    let mut encoded = String::new();
    for byte in input.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
}

fn url_decode(input: &str) -> String {
    let mut out = String::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte_val) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
            {
                out.push(byte_val as char);
                i += 3;
                continue;
            }
        } else if bytes[i] == b'+' {
            out.push(' ');
            i += 1;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn extract_code_from_http_request(req_line: &str) -> Option<String> {
    let path = req_line.split_whitespace().nth(1)?;
    let query = path.split('?').nth(1)?;
    for param in query.split('&') {
        let mut parts = param.split('=');
        if parts.next()? == "code" {
            let val = parts.next()?;
            return Some(url_decode(val));
        }
    }
    None
}

const GDRIVE_API_BASE: &str = "https://www.googleapis.com";

fn escape_drive_query(input: &str) -> String {
    input.replace('\\', "\\\\").replace('\'', "\\'")
}

pub struct ProgressStream<R, F> {
    inner: R,
    total: u64,
    sent: u64,
    last_reported: u64,
    on_progress: F,
}

impl<R: std::io::Read, F: FnMut(usize, usize)> ProgressStream<R, F> {
    pub fn new(inner: R, total: usize, on_progress: F) -> Self {
        Self::new_u64(inner, total as u64, on_progress)
    }

    pub fn new_u64(inner: R, total: u64, on_progress: F) -> Self {
        Self {
            inner,
            total,
            sent: 0,
            last_reported: 0,
            on_progress,
        }
    }
}

impl<R: std::io::Read, F: FnMut(usize, usize)> std::io::Read for ProgressStream<R, F> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.sent += n as u64;
            if self.sent - self.last_reported >= 64 * 1024 || self.sent == self.total {
                (self.on_progress)(
                    self.sent.min(usize::MAX as u64) as usize,
                    self.total.min(usize::MAX as u64) as usize,
                );
                self.last_reported = self.sent;
            }
        }
        Ok(n)
    }
}

impl GoogleDriveClient {
    pub fn config_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("LIGHTMEM_HOME") {
            let p = PathBuf::from(dir);
            if !p.as_os_str().is_empty() {
                return p;
            }
        }
        dirs::home_dir()
            .map(|h| h.join(".lightmem"))
            .unwrap_or_else(|| PathBuf::from("./.lightmem"))
    }

    pub fn credentials_path() -> PathBuf {
        Self::config_dir().join("gdrive_credentials.json")
    }

    pub fn token_path() -> PathBuf {
        Self::config_dir().join("gdrive_token.json")
    }

    pub fn folders_path() -> PathBuf {
        Self::config_dir().join("gdrive_folders.json")
    }

    /// Check if Google Drive OAuth is configured and connected
    pub fn is_connected() -> bool {
        Self::token_path().exists() && Self::credentials_path().exists()
    }

    /// Query current Google Drive authentication status
    pub fn get_status() -> Result<DriveAuthStatus> {
        let creds_path = Self::credentials_path();
        let token_path = Self::token_path();
        let folders_path = Self::folders_path();

        let credentials_present = creds_path.exists();
        let token_present = token_path.exists();

        let mut user_email = None;
        let mut scopes = Vec::new();
        let mut expires_at = None;
        let mut folders = None;

        if token_present {
            if let Ok(content) = fs::read_to_string(&token_path) {
                if let Ok(tok) = serde_json::from_str::<serde_json::Value>(&content) {
                    user_email = tok
                        .get("user_email")
                        .and_then(|e| e.as_str())
                        .map(String::from);
                    if let Some(scope_str) = tok.get("scope").and_then(|s| s.as_str()) {
                        scopes = scope_str.split_whitespace().map(String::from).collect();
                    }
                    if let Some(exp_str) = tok.get("expires_at").and_then(|e| e.as_str()) {
                        if let Ok(dt) = DateTime::parse_from_rfc3339(exp_str) {
                            expires_at = Some(dt.with_timezone(&Utc));
                        }
                    }
                }
            }
        }

        if folders_path.exists() {
            if let Ok(content) = fs::read_to_string(&folders_path) {
                if let Ok(f) = serde_json::from_str::<DriveFolders>(&content) {
                    folders = Some(f);
                }
            }
        }

        let connected = credentials_present && token_present;

        Ok(DriveAuthStatus {
            connected,
            user_email,
            scopes,
            credentials_present,
            token_present,
            expires_at,
            folders,
        })
    }

    /// Log out from Google Drive, optionally revoking OAuth tokens remotely and deleting local token/folder caches
    pub fn logout(revoke_remote: bool, purge_credentials: bool) -> Result<DriveLogoutReport> {
        let token_path = Self::token_path();
        let folders_path = Self::folders_path();
        let creds_path = Self::credentials_path();

        let mut user_email = None;
        let mut remote_revoked = false;
        let mut token_to_revoke = None;

        if token_path.exists() {
            if let Ok(content) = fs::read_to_string(&token_path) {
                if let Ok(tok) = serde_json::from_str::<GDriveTokenFile>(&content) {
                    user_email = tok.user_email;
                    token_to_revoke = tok.refresh_token.or(Some(tok.access_token));
                }
            }
        }

        if revoke_remote {
            if let Some(tok_str) = token_to_revoke {
                let params = [("token", tok_str.as_str())];
                let resp = ureq::post("https://oauth2.googleapis.com/revoke")
                    .timeout(std::time::Duration::from_secs(10))
                    .send_form(&params);
                if resp.is_ok() {
                    remote_revoked = true;
                }
            }
        }

        let mut token_removed = false;
        if token_path.exists() {
            let _ = fs::remove_file(&token_path);
            token_removed = true;
        }

        let mut folders_cache_removed = false;
        if folders_path.exists() {
            let _ = fs::remove_file(&folders_path);
            folders_cache_removed = true;
        }

        let mut credentials_removed = false;
        if purge_credentials && creds_path.exists() {
            let _ = fs::remove_file(&creds_path);
            credentials_removed = true;
        }

        Ok(DriveLogoutReport {
            user_email,
            remote_revoked,
            token_removed,
            folders_cache_removed,
            credentials_removed,
        })
    }

    /// Perform browser-based OAuth2 login with desktop loopback redirect
    pub fn login(custom_credentials_path: Option<&Path>) -> Result<DriveAuthStatus> {
        let creds_dest = Self::credentials_path();
        if let Some(src) = custom_credentials_path {
            if !src.exists() {
                anyhow::bail!("Credentials file not found at: {}", src.display());
            }
            if let Some(parent) = creds_dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(src, &creds_dest)
                .with_context(|| format!("Failed to copy credentials from {}", src.display()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&creds_dest, fs::Permissions::from_mode(0o600));
            }
        }

        if !creds_dest.exists() {
            anyhow::bail!(
                "Google Drive client credentials not found.\nProvide your OAuth credentials via:\n  lmem auth login --credentials <path/to/client_secret.json>"
            );
        }

        let cred_str = fs::read_to_string(&creds_dest)?;
        let cred_val: serde_json::Value =
            serde_json::from_str(&cred_str).with_context(|| "Invalid JSON in credentials file")?;

        let key = if cred_val.get("installed").is_some() {
            "installed"
        } else if cred_val.get("web").is_some() {
            "web"
        } else {
            anyhow::bail!(
                "Unrecognized Google OAuth client structure (expected 'installed' or 'web' root)"
            );
        };

        let client_id = cred_val[key]["client_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing client_id in credentials"))?
            .to_string();
        let client_secret = cred_val[key]["client_secret"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing client_secret in credentials"))?
            .to_string();

        let mut listener = None;
        let mut port = 8085;
        for p in [8085, 8086, 8087, 8088, 8089] {
            if let Ok(l) = std::net::TcpListener::bind(format!("127.0.0.1:{}", p)) {
                listener = Some(l);
                port = p;
                break;
            }
        }

        let listener = listener.ok_or_else(|| {
            anyhow::anyhow!(
                "Could not bind local loopback port for OAuth callback (tried 8085..8089)"
            )
        })?;
        listener.set_nonblocking(true)?;

        let redirect_uri = format!("http://localhost:{}", port);
        let scope = "https://www.googleapis.com/auth/drive.file https://www.googleapis.com/auth/userinfo.email openid";
        let auth_url = format!(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent",
            url_encode(&client_id),
            url_encode(&redirect_uri),
            url_encode(scope)
        );

        println!();
        println!("  \x1b[1;38;2;16;185;129mGoogle Drive OAuth Authorization\x1b[0m");
        println!("  Opening browser for authorization...");
        println!("  If the browser did not open automatically, copy & paste this link:");
        println!("  \x1b[38;2;96;165;250m{}\x1b[0m", auth_url);
        println!();

        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("open").arg(&auth_url).spawn();
        }

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(120);
        let mut code: Option<String> = None;

        while start.elapsed() < timeout {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    use std::io::{BufRead, BufReader, Write};
                    let mut reader = BufReader::new(&stream);
                    let mut req_line = String::new();
                    if reader.read_line(&mut req_line).is_ok() {
                        if let Some(c) = extract_code_from_http_request(&req_line) {
                            code = Some(c);
                            let body = "<!DOCTYPE html><html><head><title>LightMem Authorized</title><style>body{font-family:system-ui;background:#09090b;color:#f4f4f5;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;}div{text-align:center;padding:2rem;border:1px solid #27272a;border-radius:1rem;background:#18181b;max-width:400px;}h2{color:#10b981;margin-bottom:0.5rem;}p{color:#a1a1aa;}</style></head><body><div><h2>✓ LightMem Authorized</h2><p>Google Drive connection successful.<br>You can close this tab and return to your terminal.</p></div></body></html>";
                            let resp = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=UTF-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                body.len(),
                                body
                            );
                            let _ = stream.write_all(resp.as_bytes());
                            let _ = stream.flush();
                            break;
                        }
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(150));
                }
                Err(_) => {
                    std::thread::sleep(std::time::Duration::from_millis(150));
                }
            }
        }

        let code = code.ok_or_else(|| anyhow::anyhow!("Authentication timed out or cancelled"))?;

        let params = [
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", code.as_str()),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri.as_str()),
        ];

        let token_resp = ureq::post("https://oauth2.googleapis.com/token")
            .timeout(std::time::Duration::from_secs(15))
            .send_form(&params)
            .with_context(|| "Failed to exchange authorization code for Google tokens")?;

        let mut token_val: serde_json::Value = token_resp.into_json()?;
        let access_tok = token_val["access_token"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing access_token in Google token response"))?
            .to_string();

        let expires_in = token_val["expires_in"].as_i64().unwrap_or(3600);
        let expires_at = Utc::now() + Duration::seconds(expires_in - 60);

        let mut user_email = "Google Drive User".to_string();
        if let Ok(user_resp) = ureq::get("https://www.googleapis.com/oauth2/v2/userinfo")
            .set("Authorization", &format!("Bearer {}", access_tok))
            .timeout(std::time::Duration::from_secs(10))
            .call()
        {
            if let Ok(user_val) = user_resp.into_json::<serde_json::Value>() {
                if let Some(email) = user_val["email"].as_str() {
                    user_email = email.to_string();
                }
            }
        }

        if let Some(map) = token_val.as_object_mut() {
            map.insert(
                "user_email".to_string(),
                serde_json::Value::String(user_email.clone()),
            );
            map.insert(
                "expires_at".to_string(),
                serde_json::Value::String(expires_at.to_rfc3339()),
            );
        }

        let token_path = Self::token_path();
        if let Some(p) = token_path.parent() {
            fs::create_dir_all(p)?;
        }
        fs::write(&token_path, serde_json::to_string_pretty(&token_val)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&token_path, fs::Permissions::from_mode(0o600));
        }

        let client = Self::try_load()?
            .ok_or_else(|| anyhow::anyhow!("Failed to reload client after auth"))?;
        let folders = client.ensure_folders()?;

        let status = DriveAuthStatus {
            connected: true,
            user_email: Some(user_email),
            scopes: vec![
                "https://www.googleapis.com/auth/drive.file".to_string(),
                "https://www.googleapis.com/auth/userinfo.email".to_string(),
            ],
            credentials_present: true,
            token_present: true,
            expires_at: Some(expires_at),
            folders: Some(folders),
        };

        Ok(status)
    }

    /// Try loading the authenticated Google Drive client from ~/.lightmem
    pub fn try_load() -> Result<Option<Self>> {
        let cred_path = Self::credentials_path();
        let token_path = Self::token_path();

        if !cred_path.exists() || !token_path.exists() {
            return Ok(None);
        }

        let cred_str = fs::read_to_string(&cred_path)
            .with_context(|| format!("Failed to read {}", cred_path.display()))?;
        let cred_val: serde_json::Value = serde_json::from_str(&cred_str)
            .with_context(|| "Invalid JSON in gdrive_credentials.json")?;

        let key = if cred_val.get("installed").is_some() {
            "installed"
        } else if cred_val.get("web").is_some() {
            "web"
        } else {
            anyhow::bail!("Unrecognized Google credentials JSON structure");
        };

        let client_id = cred_val[key]["client_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing client_id in credentials"))?
            .to_string();
        let client_secret = cred_val[key]["client_secret"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing client_secret in credentials"))?
            .to_string();

        let token_str = fs::read_to_string(&token_path)
            .with_context(|| format!("Failed to read {}", token_path.display()))?;
        let token_file: GDriveTokenFile = serde_json::from_str(&token_str)
            .with_context(|| "Invalid JSON in gdrive_token.json")?;

        let refresh_token = token_file
            .refresh_token
            .ok_or_else(|| anyhow::anyhow!("Missing refresh_token in gdrive_token.json"))?;
        let user_email = token_file
            .user_email
            .unwrap_or_else(|| "Google Drive User".to_string());

        let expires_at = token_file.expires_at.unwrap_or_else(|| {
            let secs = token_file.expires_in.unwrap_or(3600);
            Utc::now() + Duration::seconds(secs - 60)
        });

        let client = Self {
            client_id,
            client_secret,
            refresh_token,
            user_email,
            token_cache: Mutex::new(Some((token_file.access_token, expires_at))),
        };

        Ok(Some(client))
    }

    /// Retrieve a valid access token, silently refreshing with refresh_token if expired
    pub fn get_access_token(&self) -> Result<String> {
        let mut cache = self.token_cache.lock().unwrap();
        let now = Utc::now();

        if let Some((ref tok, exp)) = *cache {
            if exp > now + Duration::seconds(60) {
                return Ok(tok.clone());
            }
        }

        // Silent refresh
        let params = [
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.client_secret.as_str()),
            ("refresh_token", self.refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ];

        let resp = ureq::post("https://oauth2.googleapis.com/token")
            .timeout(std::time::Duration::from_secs(15))
            .send_form(&params)
            .with_context(|| "Failed to refresh Google OAuth access token")?;

        let new_token_val: serde_json::Value = resp.into_json()?;
        let new_tok = new_token_val["access_token"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing access_token in token refresh response"))?
            .to_string();
        let expires_in = new_token_val["expires_in"].as_i64().unwrap_or(3600);
        let new_exp = Utc::now() + Duration::seconds(expires_in - 60);

        *cache = Some((new_tok.clone(), new_exp));

        // Persist updated token file
        let token_path = Self::token_path();
        if let Ok(content) = fs::read_to_string(&token_path) {
            if let Ok(mut json_obj) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(map) = json_obj.as_object_mut() {
                    map.insert(
                        "access_token".to_string(),
                        serde_json::Value::String(new_tok.clone()),
                    );
                    map.insert(
                        "expires_at".to_string(),
                        serde_json::Value::String(new_exp.to_rfc3339()),
                    );
                }
                let _ = fs::write(&token_path, serde_json::to_string_pretty(&json_obj)?);
            }
        }

        Ok(new_tok)
    }

    /// Ensure the LightMem, LightMem/blobs, and LightMem/snapshots folders exist on Google Drive
    pub fn ensure_folders(&self) -> Result<DriveFolders> {
        let cache_path = Self::folders_path();
        if cache_path.exists() {
            if let Ok(data) = fs::read_to_string(&cache_path) {
                if let Ok(folders) = serde_json::from_str::<DriveFolders>(&data) {
                    return Ok(folders);
                }
            }
        }

        let token = self.get_access_token()?;
        let auth_hdr = format!("Bearer {}", token);

        // 1. Root LightMem folder
        let root_id = self.find_or_create_folder("LightMem", None, &auth_hdr)?;
        // 2. Blobs subfolder
        let blobs_id = self.find_or_create_folder("blobs", Some(&root_id), &auth_hdr)?;
        // 3. Snapshots subfolder
        let snapshots_id = self.find_or_create_folder("snapshots", Some(&root_id), &auth_hdr)?;

        let folders = DriveFolders {
            root_id,
            blobs_id,
            snapshots_id,
        };

        if let Ok(json_str) = serde_json::to_string_pretty(&folders) {
            let _ = fs::write(&cache_path, json_str);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&cache_path, fs::Permissions::from_mode(0o600));
            }
        }

        Ok(folders)
    }

    fn find_or_create_folder(
        &self,
        name: &str,
        parent_id: Option<&str>,
        auth_hdr: &str,
    ) -> Result<String> {
        let q = if let Some(pid) = parent_id {
            format!(
                "mimeType = 'application/vnd.google-apps.folder' and name = '{}' and '{}' in parents and trashed = false",
                name, pid
            )
        } else {
            format!(
                "mimeType = 'application/vnd.google-apps.folder' and name = '{}' and trashed = false",
                name
            )
        };

        let search_url = format!(
            "https://www.googleapis.com/drive/v3/files?q={}&fields=files(id,name)",
            url_encode(&q)
        );

        let resp = ureq::get(&search_url)
            .set("Authorization", auth_hdr)
            .timeout(std::time::Duration::from_secs(15))
            .call()
            .with_context(|| format!("Failed to search Google Drive for folder '{}'", name))?;

        let res_json: serde_json::Value = resp.into_json()?;
        if let Some(files) = res_json.get("files").and_then(|f| f.as_array()) {
            if let Some(first) = files.first() {
                if let Some(id) = first.get("id").and_then(|i| i.as_str()) {
                    return Ok(id.to_string());
                }
            }
        }

        // Create folder
        let mut body = serde_json::json!({
            "name": name,
            "mimeType": "application/vnd.google-apps.folder"
        });
        if let Some(pid) = parent_id {
            body["parents"] = serde_json::json!([pid]);
        }

        let create_resp = ureq::post("https://www.googleapis.com/drive/v3/files")
            .set("Authorization", auth_hdr)
            .set("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(15))
            .send_json(body)
            .with_context(|| format!("Failed to create Google Drive folder '{}'", name))?;

        let create_json: serde_json::Value = create_resp.into_json()?;
        let id = create_json["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Created folder missing ID"))?
            .to_string();

        Ok(id)
    }

    /// List all existing CAS blob filenames in the Google Drive blobs folder
    pub fn list_existing_blobs(&self, blobs_folder_id: &str) -> Result<HashSet<String>> {
        Ok(self
            .list_existing_blobs_with_ids(blobs_folder_id)?
            .into_keys()
            .collect())
    }

    /// List all existing CAS blob filenames mapped to their Google Drive file IDs.
    pub fn list_existing_blobs_with_ids(
        &self,
        blobs_folder_id: &str,
    ) -> Result<HashMap<String, String>> {
        self.list_existing_blobs_with_ids_with_base(GDRIVE_API_BASE, blobs_folder_id)
    }

    pub(crate) fn list_existing_blobs_with_ids_with_base(
        &self,
        api_base: &str,
        blobs_folder_id: &str,
    ) -> Result<HashMap<String, String>> {
        let token = self.get_access_token()?;
        let auth_hdr = format!("Bearer {}", token);
        let base = api_base.trim_end_matches('/');

        let q = format!(
            "'{}' in parents and trashed = false",
            escape_drive_query(blobs_folder_id)
        );
        let mut out = HashMap::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut url = format!(
                "{}/drive/v3/files?q={}&fields=nextPageToken,files(id,name)&pageSize=1000",
                base,
                url_encode(&q)
            );
            if let Some(ref pt) = page_token {
                url.push_str(&format!("&pageToken={}", url_encode(pt)));
            }

            let resp = ureq::get(&url)
                .set("Authorization", &auth_hdr)
                .timeout(std::time::Duration::from_secs(20))
                .call()
                .with_context(|| "Failed to list Google Drive CAS blobs")?;

            let val: serde_json::Value = resp.into_json()?;
            if let Some(files) = val.get("files").and_then(|f| f.as_array()) {
                for file in files {
                    if let (Some(name), Some(id)) = (
                        file.get("name").and_then(|n| n.as_str()),
                        file.get("id").and_then(|i| i.as_str()),
                    ) {
                        out.insert(name.to_string(), id.to_string());
                    }
                }
            }

            page_token = val
                .get("nextPageToken")
                .and_then(|p| p.as_str())
                .map(String::from);
            if page_token.is_none() {
                break;
            }
        }

        Ok(out)
    }

    pub(crate) fn upload_multipart_reader_with_base<R: Read, F: FnMut(usize, usize)>(
        &self,
        api_base: &str,
        name: &str,
        parent_id: &str,
        mime_type: &str,
        payload_len: u64,
        payload_reader: R,
        on_progress: F,
    ) -> Result<String> {
        let token = self.get_access_token()?;
        let auth_hdr = format!("Bearer {}", token);
        let base = api_base.trim_end_matches('/');

        let boundary = "-------314159265358979323846LightMemBoundary";
        let metadata = serde_json::json!({
            "name": name,
            "parents": [parent_id]
        });

        let header = format!(
            "--{}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{}\r\n--{}\r\nContent-Type: {}\r\n\r\n",
            boundary, metadata, boundary, mime_type
        )
        .into_bytes();
        let footer = format!("\r\n--{}--\r\n", boundary).into_bytes();

        let total_bytes = header.len() as u64 + payload_len + footer.len() as u64;
        // Dynamic timeout: at least 600 seconds, plus time based on 20 KB/s transfer floor
        let timeout_secs = std::cmp::max(600, (payload_len / (20 * 1024)) + 120);

        let chained = Cursor::new(header)
            .chain(payload_reader)
            .chain(Cursor::new(footer));
        let stream = ProgressStream::new_u64(chained, total_bytes, on_progress);

        let url = format!("{}/upload/drive/v3/files?uploadType=multipart", base);
        let resp = ureq::post(&url)
            .set("Authorization", &auth_hdr)
            .set(
                "Content-Type",
                &format!("multipart/related; boundary={}", boundary),
            )
            .set("Content-Length", &total_bytes.to_string())
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .send(stream)
            .with_context(|| format!("Failed to upload '{}' to Google Drive", name))?;

        let res_json: serde_json::Value = resp.into_json()?;
        let id = res_json["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Upload response missing file ID"))?
            .to_string();

        Ok(id)
    }

    pub(crate) fn patch_media_reader_with_base<R: Read, F: FnMut(usize, usize)>(
        &self,
        api_base: &str,
        file_id: &str,
        name: &str,
        mime_type: &str,
        payload_len: u64,
        payload_reader: R,
        on_progress: F,
    ) -> Result<String> {
        let token = self.get_access_token()?;
        let auth_hdr = format!("Bearer {}", token);
        let base = api_base.trim_end_matches('/');

        let update_url = format!(
            "{}/upload/drive/v3/files/{}?uploadType=media",
            base, file_id
        );
        let timeout_secs = std::cmp::max(600, (payload_len / (20 * 1024)) + 120);
        let stream = ProgressStream::new_u64(payload_reader, payload_len, on_progress);

        let update_resp = ureq::patch(&update_url)
            .set("Authorization", &auth_hdr)
            .set("Content-Type", mime_type)
            .set("Content-Length", &payload_len.to_string())
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .send(stream)
            .with_context(|| format!("Failed to update '{}' on Google Drive", name))?;

        let update_json: serde_json::Value = update_resp.into_json()?;
        let id = update_json["id"].as_str().unwrap_or(file_id).to_string();
        Ok(id)
    }

    /// Upload a binary payload via multipart upload to a Google Drive folder
    pub fn upload_file_multipart(
        &self,
        name: &str,
        parent_id: &str,
        mime_type: &str,
        data: &[u8],
    ) -> Result<String> {
        self.upload_file_multipart_with_progress(name, parent_id, mime_type, data, |_, _| {})
    }

    /// Upload a binary payload via multipart upload to a Google Drive folder with progress reporting
    pub fn upload_file_multipart_with_progress<F>(
        &self,
        name: &str,
        parent_id: &str,
        mime_type: &str,
        data: &[u8],
        on_progress: F,
    ) -> Result<String>
    where
        F: FnMut(usize, usize),
    {
        self.upload_multipart_reader_with_base(
            GDRIVE_API_BASE,
            name,
            parent_id,
            mime_type,
            data.len() as u64,
            Cursor::new(data),
            on_progress,
        )
    }

    /// Stream a file directly from disk via multipart upload to a Google Drive folder
    pub fn upload_file_multipart_from_path_with_progress<F>(
        &self,
        name: &str,
        parent_id: &str,
        mime_type: &str,
        file_path: &Path,
        on_progress: F,
    ) -> Result<String>
    where
        F: FnMut(usize, usize),
    {
        self.upload_file_multipart_from_path_with_base(
            GDRIVE_API_BASE,
            name,
            parent_id,
            mime_type,
            file_path,
            on_progress,
        )
    }

    pub(crate) fn upload_file_multipart_from_path_with_base<F>(
        &self,
        api_base: &str,
        name: &str,
        parent_id: &str,
        mime_type: &str,
        file_path: &Path,
        on_progress: F,
    ) -> Result<String>
    where
        F: FnMut(usize, usize),
    {
        let file = fs::File::open(file_path).with_context(|| {
            format!(
                "Failed to open file for Drive upload: {}",
                file_path.display()
            )
        })?;
        let payload_len = file
            .metadata()
            .with_context(|| format!("Failed to inspect size of {}", file_path.display()))?
            .len();
        let reader = BufReader::with_capacity(64 * 1024, file);
        self.upload_multipart_reader_with_base(
            api_base,
            name,
            parent_id,
            mime_type,
            payload_len,
            reader,
            on_progress,
        )
    }

    /// Upload or update an existing file (e.g. vault_latest.db, manifest.json)
    pub fn upload_or_update_file(
        &self,
        name: &str,
        parent_id: &str,
        mime_type: &str,
        data: &[u8],
    ) -> Result<String> {
        self.upload_or_update_file_with_progress(name, parent_id, mime_type, data, |_, _| {})
    }

    /// Upload or update an existing file with progress reporting
    pub fn upload_or_update_file_with_progress<F>(
        &self,
        name: &str,
        parent_id: &str,
        mime_type: &str,
        data: &[u8],
        on_progress: F,
    ) -> Result<String>
    where
        F: FnMut(usize, usize),
    {
        if let Some(file_id) = self.find_file_with_base(GDRIVE_API_BASE, name, parent_id)? {
            self.patch_media_reader_with_base(
                GDRIVE_API_BASE,
                &file_id,
                name,
                mime_type,
                data.len() as u64,
                Cursor::new(data),
                on_progress,
            )
        } else {
            self.upload_file_multipart_with_progress(name, parent_id, mime_type, data, on_progress)
        }
    }

    /// Stream an upload or in-place update directly from a file on disk without loading it into RAM.
    pub fn upload_or_update_file_from_path_with_progress<F>(
        &self,
        name: &str,
        parent_id: &str,
        mime_type: &str,
        file_path: &Path,
        on_progress: F,
    ) -> Result<String>
    where
        F: FnMut(usize, usize),
    {
        self.upload_or_update_file_from_path_with_base(
            GDRIVE_API_BASE,
            name,
            parent_id,
            mime_type,
            file_path,
            on_progress,
        )
    }

    pub(crate) fn upload_or_update_file_from_path_with_base<F>(
        &self,
        api_base: &str,
        name: &str,
        parent_id: &str,
        mime_type: &str,
        file_path: &Path,
        on_progress: F,
    ) -> Result<String>
    where
        F: FnMut(usize, usize),
    {
        if let Some(file_id) = self.find_file_with_base(api_base, name, parent_id)? {
            let file = fs::File::open(file_path).with_context(|| {
                format!(
                    "Failed to open file for Drive update: {}",
                    file_path.display()
                )
            })?;
            let payload_len = file
                .metadata()
                .with_context(|| format!("Failed to inspect size of {}", file_path.display()))?
                .len();
            let reader = BufReader::with_capacity(64 * 1024, file);
            self.patch_media_reader_with_base(
                api_base,
                &file_id,
                name,
                mime_type,
                payload_len,
                reader,
                on_progress,
            )
        } else {
            self.upload_file_multipart_from_path_with_base(
                api_base,
                name,
                parent_id,
                mime_type,
                file_path,
                on_progress,
            )
        }
    }

    /// Download a file's binary content from Google Drive
    pub fn download_file(&self, file_id: &str) -> Result<Vec<u8>> {
        let token = self.get_access_token()?;
        let auth_hdr = format!("Bearer {}", token);
        let url = format!("{}/drive/v3/files/{}?alt=media", GDRIVE_API_BASE, file_id);

        let resp = ureq::get(&url)
            .set("Authorization", &auth_hdr)
            .timeout(std::time::Duration::from_secs(300))
            .call()
            .with_context(|| format!("Failed to download file '{}' from Google Drive", file_id))?;

        let mut bytes = Vec::new();
        resp.into_reader().read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    /// Stream a Google Drive file download directly to `dest_path` without buffering the payload in RAM.
    pub fn download_file_to_path(&self, file_id: &str, dest_path: &Path) -> Result<u64> {
        self.download_file_to_path_with_base(GDRIVE_API_BASE, file_id, dest_path)
    }

    pub(crate) fn download_file_to_path_with_base(
        &self,
        api_base: &str,
        file_id: &str,
        dest_path: &Path,
    ) -> Result<u64> {
        let token = self.get_access_token()?;
        let auth_hdr = format!("Bearer {}", token);
        let base = api_base.trim_end_matches('/');
        let url = format!("{}/drive/v3/files/{}?alt=media", base, file_id);

        let resp = ureq::get(&url)
            .set("Authorization", &auth_hdr)
            .timeout(std::time::Duration::from_secs(1800))
            .call()
            .with_context(|| format!("Failed to download file '{}' from Google Drive", file_id))?;

        let mut reader = resp.into_reader();
        let file = fs::File::create(dest_path)
            .with_context(|| format!("Failed to create download target {}", dest_path.display()))?;
        let mut writer = BufWriter::with_capacity(64 * 1024, file);
        let written = std::io::copy(&mut reader, &mut writer).with_context(|| {
            format!(
                "Failed to stream Google Drive file '{}' to {}",
                file_id,
                dest_path.display()
            )
        })?;
        writer.flush()?;
        Ok(written)
    }

    /// Search for a file by name inside a parent folder
    pub fn find_file(&self, name: &str, parent_id: &str) -> Result<Option<String>> {
        self.find_file_with_base(GDRIVE_API_BASE, name, parent_id)
    }

    pub(crate) fn find_file_with_base(
        &self,
        api_base: &str,
        name: &str,
        parent_id: &str,
    ) -> Result<Option<String>> {
        let token = self.get_access_token()?;
        let auth_hdr = format!("Bearer {}", token);
        let base = api_base.trim_end_matches('/');
        let q = format!(
            "name = '{}' and '{}' in parents and trashed = false",
            escape_drive_query(name),
            escape_drive_query(parent_id)
        );
        let url = format!(
            "{}/drive/v3/files?q={}&fields=files(id)",
            base,
            url_encode(&q)
        );

        let resp = ureq::get(&url)
            .set("Authorization", &auth_hdr)
            .timeout(std::time::Duration::from_secs(15))
            .call()
            .with_context(|| format!("Failed to search Google Drive for '{}'", name))?;

        let res_json: serde_json::Value = resp.into_json()?;
        let file_id = res_json
            .get("files")
            .and_then(|f| f.as_array())
            .and_then(|arr| arr.first())
            .and_then(|item| item.get("id"))
            .and_then(|id| id.as_str())
            .map(String::from);

        Ok(file_id)
    }
}

#[cfg(test)]
mod streaming_http_tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    fn mock_client() -> GoogleDriveClient {
        GoogleDriveClient {
            client_id: "test-client".to_string(),
            client_secret: "test-secret".to_string(),
            refresh_token: "test-refresh".to_string(),
            user_email: "test@example.com".to_string(),
            token_cache: Mutex::new(Some((
                "ya29.mock-access-token".to_string(),
                Utc::now() + Duration::hours(1),
            ))),
        }
    }

    #[derive(Debug)]
    struct CapturedRequest {
        method_path: String,
        headers: HashMap<String, String>,
        body: Vec<u8>,
    }

    fn read_http_request(stream: &mut std::net::TcpStream) -> CapturedRequest {
        let mut reader = BufReader::new(stream);
        let mut request_line = String::new();
        reader.read_line(&mut request_line).unwrap();
        let method_path = request_line
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ");

        let mut headers = HashMap::new();
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                break;
            }
            if let Some((k, v)) = trimmed.split_once(':') {
                let key = k.trim().to_ascii_lowercase();
                let val = v.trim().to_string();
                if key == "content-length" {
                    content_length = val.parse::<usize>().unwrap_or(0);
                }
                headers.insert(key, val);
            }
        }

        let mut body = vec![0u8; content_length];
        if content_length > 0 {
            reader.read_exact(&mut body).unwrap();
        }
        CapturedRequest {
            method_path,
            headers,
            body,
        }
    }

    #[test]
    fn test_stream_multipart_and_patch_from_path_with_progress() {
        let dir = tempfile::tempdir().unwrap();
        let src_path = dir.path().join("snapshot.db");
        let payload: Vec<u8> = (0..150_000).map(|i| ((i * 17 + 3) % 251) as u8).collect();
        fs::write(&src_path, &payload).unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let api_base = format!("http://{}", addr);

        let handle = std::thread::spawn(move || {
            let mut captured = Vec::new();
            for idx in 0..4 {
                let (mut stream, _) = listener.accept().unwrap();
                let req = read_http_request(&mut stream);
                let resp_json = if req.method_path.starts_with("GET /drive/v3/files") {
                    if idx == 2 {
                        r#"{"files":[{"id":"existing-db-id"}]}"#.to_string()
                    } else {
                        r#"{"files":[]}"#.to_string()
                    }
                } else if req.method_path.starts_with("PATCH ") {
                    r#"{"id":"existing-db-id"}"#.to_string()
                } else {
                    r#"{"id":"created-db-id"}"#.to_string()
                };
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    resp_json.len(),
                    resp_json
                );
                stream.write_all(resp.as_bytes()).unwrap();
                stream.flush().unwrap();
                captured.push(req);
            }
            captured
        });

        let client = mock_client();

        // 1. Multipart upload (not yet on Drive: GET -> empty -> POST multipart)
        let mut create_progress = Vec::new();
        let created_id = client
            .upload_or_update_file_from_path_with_base(
                &api_base,
                "vault_latest.db",
                "snap-folder",
                "application/x-sqlite3",
                &src_path,
                |sent, total| create_progress.push((sent, total)),
            )
            .unwrap();
        assert_eq!(created_id, "created-db-id");
        assert!(create_progress.len() >= 2);
        let last_create = *create_progress.last().unwrap();
        assert_eq!(last_create.0, last_create.1);
        assert!(last_create.1 > payload.len());

        // 2. Patch update (exists on Drive: GET -> id -> PATCH media)
        let mut patch_progress = Vec::new();
        let patched_id = client
            .upload_or_update_file_from_path_with_base(
                &api_base,
                "vault_latest.db",
                "snap-folder",
                "application/x-sqlite3",
                &src_path,
                |sent, total| patch_progress.push((sent, total)),
            )
            .unwrap();
        assert_eq!(patched_id, "existing-db-id");
        assert_eq!(
            patch_progress.last().copied(),
            Some((payload.len(), payload.len()))
        );

        let captured = handle.join().unwrap();
        assert_eq!(captured.len(), 4);
        // Request 1: POST multipart containing verbatim payload
        assert_eq!(
            captured[1].method_path,
            "POST /upload/drive/v3/files?uploadType=multipart"
        );
        assert_eq!(
            captured[1].headers.get("content-length").unwrap(),
            &captured[1].body.len().to_string()
        );
        assert!(captured[1]
            .body
            .windows(payload.len())
            .any(|w| w == payload.as_slice()));

        // Request 3: PATCH media equals exact payload bytes
        assert_eq!(
            captured[3].method_path,
            "PATCH /upload/drive/v3/files/existing-db-id?uploadType=media"
        );
        assert_eq!(
            captured[3].headers.get("content-length").unwrap(),
            &payload.len().to_string()
        );
        assert_eq!(captured[3].body, payload);
    }

    #[test]
    fn test_list_blobs_paginates_and_download_streams_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        let dest_path = dir.path().join("downloaded.bin");
        let payload: Vec<u8> = (0..140_000).map(|i| ((i * 29 + 11) % 251) as u8).collect();
        let payload_clone = payload.clone();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let api_base = format!("http://{}", addr);

        let handle = std::thread::spawn(move || {
            for idx in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let req = read_http_request(&mut stream);
                if idx == 0 {
                    assert!(!req.method_path.contains("pageToken"));
                    let body =
                        r#"{"nextPageToken":"page2","files":[{"id":"id-a","name":"shaA.png"}]}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    stream.write_all(resp.as_bytes()).unwrap();
                } else if idx == 1 {
                    assert!(req.method_path.contains("pageToken=page2"));
                    let body = r#"{"files":[{"id":"id-b","name":"shaB.mp4"}]}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    stream.write_all(resp.as_bytes()).unwrap();
                } else {
                    assert_eq!(req.method_path, "GET /drive/v3/files/id-b?alt=media");
                    let hdr = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        payload_clone.len()
                    );
                    stream.write_all(hdr.as_bytes()).unwrap();
                    stream.write_all(&payload_clone).unwrap();
                }
                stream.flush().unwrap();
            }
        });

        let client = mock_client();
        let map = client
            .list_existing_blobs_with_ids_with_base(&api_base, "blobs-dir")
            .unwrap();
        assert_eq!(map.get("shaA.png").map(String::as_str), Some("id-a"));
        assert_eq!(map.get("shaB.mp4").map(String::as_str), Some("id-b"));

        let written = client
            .download_file_to_path_with_base(&api_base, "id-b", &dest_path)
            .unwrap();
        assert_eq!(written, payload.len() as u64);
        assert_eq!(fs::read(&dest_path).unwrap(), payload);

        handle.join().unwrap();
    }
}
