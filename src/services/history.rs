use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandHistoryEntry {
    pub id: i64,
    pub command: String,
    pub subcommand: String,
    pub status: String,
    pub duration_ms: u64,
    pub error_message: Option<String>,
    pub created_at: String,
}

pub struct HistoryService;

impl HistoryService {
    pub fn db_path() -> PathBuf {
        crate::models::config::LightMemConfig::config_dir().join("history.db")
    }

    pub fn open_conn() -> Result<Connection> {
        Self::open_conn_at(&Self::db_path())
    }

    pub fn open_conn_at(path: &Path) -> Result<Connection> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        let _ = conn.busy_timeout(std::time::Duration::from_secs(5));
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "synchronous", "NORMAL");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS command_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                command TEXT NOT NULL,
                subcommand TEXT NOT NULL,
                status TEXT NOT NULL,
                duration_ms INTEGER NOT NULL,
                error_message TEXT,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_history_created_at ON command_history(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_history_subcommand ON command_history(subcommand);",
        )?;
        Ok(conn)
    }

    /// Record a command execution into history.
    /// Never panics or returns error to the caller if recording fails.
    pub fn record(
        command: &str,
        subcommand: &str,
        status: &str,
        duration_ms: u64,
        error_message: Option<&str>,
    ) {
        if std::env::var("LIGHTMEM_NO_HISTORY")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
        {
            return;
        }
        let _ = Self::record_at(
            &Self::db_path(),
            command,
            subcommand,
            status,
            duration_ms,
            error_message,
        );
    }

    fn record_at(
        path: &Path,
        command: &str,
        subcommand: &str,
        status: &str,
        duration_ms: u64,
        error_message: Option<&str>,
    ) -> Result<()> {
        let conn = Self::open_conn_at(path)?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO command_history (command, subcommand, status, duration_ms, error_message, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![command, subcommand, status, duration_ms as i64, error_message, now],
        )?;

        // Auto-prune oldest if history exceeds 10,000 items
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM command_history", [], |r| r.get(0))
            .unwrap_or(0);
        if count > 10_000 {
            let _ = conn.execute(
                "DELETE FROM command_history WHERE id NOT IN (
                    SELECT id FROM command_history ORDER BY id DESC LIMIT 10000
                )",
                [],
            );
        }
        Ok(())
    }

    /// Fetch command history matching optional search filter, status, and limit.
    pub fn list(
        query: Option<&str>,
        status: Option<&str>,
        limit: usize,
    ) -> Result<(Vec<CommandHistoryEntry>, usize)> {
        Self::list_at(&Self::db_path(), query, status, limit)
    }

    fn list_at(
        path: &Path,
        query: Option<&str>,
        status: Option<&str>,
        limit: usize,
    ) -> Result<(Vec<CommandHistoryEntry>, usize)> {
        if !path.exists() {
            return Ok((Vec::new(), 0));
        }
        let conn = Connection::open(path)?;
        let _ = conn.busy_timeout(std::time::Duration::from_secs(3));

        let table_exists: bool = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='command_history'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);

        if !table_exists {
            return Ok((Vec::new(), 0));
        }

        let total_count: usize = conn
            .query_row("SELECT COUNT(*) FROM command_history", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap_or(0) as usize;

        let mut sql = "SELECT id, command, subcommand, status, duration_ms, error_message, created_at FROM command_history WHERE 1=1".to_string();
        let mut param_vals: Vec<rusqlite::types::Value> = Vec::new();

        if let Some(q) = query.map(str::trim).filter(|s| !s.is_empty()) {
            sql.push_str(" AND (command LIKE ? OR subcommand LIKE ?)");
            let pattern = format!("%{q}%");
            param_vals.push(pattern.clone().into());
            param_vals.push(pattern.into());
        }

        if let Some(s) = status.map(str::trim).filter(|s| !s.is_empty()) {
            let norm = if s.eq_ignore_ascii_case("ok") || s.eq_ignore_ascii_case("success") {
                "success"
            } else if s.eq_ignore_ascii_case("err")
                || s.eq_ignore_ascii_case("error")
                || s.eq_ignore_ascii_case("fail")
                || s.eq_ignore_ascii_case("failed")
            {
                "error"
            } else {
                s
            };
            sql.push_str(" AND status = ?");
            param_vals.push(norm.to_string().into());
        }

        sql.push_str(" ORDER BY id DESC LIMIT ?");
        param_vals.push((limit as i64).into());

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(param_vals.iter()), |row| {
            let duration: i64 = row.get(4)?;
            Ok(CommandHistoryEntry {
                id: row.get(0)?,
                command: row.get(1)?,
                subcommand: row.get(2)?,
                status: row.get(3)?,
                duration_ms: duration.max(0) as u64,
                error_message: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;

        let mut entries = Vec::new();
        for r in rows {
            entries.push(r?);
        }
        Ok((entries, total_count))
    }

    /// Clear all command history.
    pub fn clear() -> Result<usize> {
        Self::clear_at(&Self::db_path())
    }

    fn clear_at(path: &Path) -> Result<usize> {
        if !path.exists() {
            return Ok(0);
        }
        let conn = Connection::open(path)?;
        let deleted = conn.execute("DELETE FROM command_history", [])?;
        let _ = conn.execute("VACUUM", []);
        Ok(deleted)
    }

    fn is_secret_flag(flag: &str) -> bool {
        matches!(
            flag,
            "--api-key"
                | "--ollama-api-key"
                | "--openai-api-key"
                | "--model-api-key"
                | "--remote-api-key"
                | "--embedding-api-key"
                | "--server-key"
                | "--key"
        )
    }

    /// Sanitize CLI arguments to redact any secrets / API keys before storing.
    pub fn sanitize_args(args: &[String]) -> String {
        if args.is_empty() {
            return "lmem".to_string();
        }
        let mut sanitized = Vec::new();
        sanitized.push("lmem".to_string());

        let mut skip_next = false;
        for arg in args.iter().skip(1) {
            if skip_next {
                sanitized.push("[REDACTED]".to_string());
                skip_next = false;
                continue;
            }
            if Self::is_secret_flag(arg) {
                sanitized.push(arg.clone());
                skip_next = true;
            } else if let Some((flag, _val)) = arg.split_once('=') {
                if Self::is_secret_flag(flag) {
                    sanitized.push(format!("{flag}=[REDACTED]"));
                } else if arg.contains(' ') || arg.contains('\t') {
                    sanitized.push(format!("\"{}\"", arg.replace('\"', "\\\"")));
                } else {
                    sanitized.push(arg.clone());
                }
            } else if arg.contains(' ') || arg.contains('\t') || arg.contains('\n') {
                sanitized.push(format!("\"{}\"", arg.replace('\"', "\\\"")));
            } else {
                sanitized.push(arg.clone());
            }
        }
        sanitized.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_args_redacts_keys() {
        let raw = vec![
            "/target/release/lmem".to_string(),
            "config".to_string(),
            "--api-key".to_string(),
            "sk-123456789".to_string(),
            "--endpoint".to_string(),
            "https://api.openai.com".to_string(),
        ];
        let sanitized = HistoryService::sanitize_args(&raw);
        assert_eq!(
            sanitized,
            "lmem config --api-key [REDACTED] --endpoint https://api.openai.com"
        );

        let raw_eq = vec![
            "lmem".to_string(),
            "config".to_string(),
            "--api-key=secret_pass".to_string(),
            "--remote-api-key".to_string(),
            "remote_tok".to_string(),
            "--embedding-api-key=emb_tok".to_string(),
            "--server-key".to_string(),
            "srv_tok".to_string(),
            "--key".to_string(),
            "mod_tok".to_string(),
        ];
        assert_eq!(
            HistoryService::sanitize_args(&raw_eq),
            "lmem config --api-key=[REDACTED] --remote-api-key [REDACTED] --embedding-api-key=[REDACTED] --server-key [REDACTED] --key [REDACTED]"
        );

        let raw_quotes = vec![
            "lmem".to_string(),
            "remember".to_string(),
            "hello world test".to_string(),
        ];
        assert_eq!(
            HistoryService::sanitize_args(&raw_quotes),
            "lmem remember \"hello world test\""
        );
    }

    #[test]
    fn test_record_list_filter_and_clear() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("history.db");

        HistoryService::record_at(
            &db_path,
            "lmem remember test",
            "remember",
            "success",
            12,
            None,
        )
        .unwrap();
        HistoryService::record_at(&db_path, "lmem recall test", "recall", "success", 8, None)
            .unwrap();
        HistoryService::record_at(
            &db_path,
            "lmem answer test",
            "answer",
            "error",
            200,
            Some("Failed"),
        )
        .unwrap();

        let (all, total) = HistoryService::list_at(&db_path, None, None, 10).unwrap();
        assert_eq!(total, 3);
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].subcommand, "answer");
        assert_eq!(all[0].status, "error");

        let (recalls, r_tot) = HistoryService::list_at(&db_path, Some("recall"), None, 10).unwrap();
        assert_eq!(r_tot, 3);
        assert_eq!(recalls.len(), 1);
        assert_eq!(recalls[0].subcommand, "recall");

        let (errs, _) = HistoryService::list_at(&db_path, None, Some("error"), 10).unwrap();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].subcommand, "answer");

        let cleared = HistoryService::clear_at(&db_path).unwrap();
        assert_eq!(cleared, 3);

        let (empty, total_after) = HistoryService::list_at(&db_path, None, None, 10).unwrap();
        assert_eq!(total_after, 0);
        assert!(empty.is_empty());
    }
}
