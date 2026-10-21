use anyhow::{Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const SKILL_MD_CONTENT: &str = include_str!("../../SKILL.md");

#[derive(Debug, Clone, Serialize)]
pub struct PlatformInfo {
    pub id: String,
    pub name: String,
    pub detected: bool,
    pub target_file: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectResult {
    pub platform: String,
    pub target_file: PathBuf,
    pub created: bool,
    pub message: String,
}

pub struct ConnectService;

impl ConnectService {
    /// Install a harness binding that always selects the registered scope.
    pub fn connect_bound(
        connection: &crate::models::ResolvedConnection,
        platform: Option<&str>,
        workspace: bool,
        custom_path: Option<&Path>,
        mcp: bool,
    ) -> Result<Vec<ConnectResult>> {
        let id = &connection.context.connection_id;
        let scope = serde_json::to_value(&connection.context.scope)?;
        let scope = scope.as_str().unwrap();
        let registry_dir = connection.registry_dir.to_string_lossy();
        let shell_registry = registry_dir.replace('\'', "'\"'\"'");
        let prefix = format!(
            "LIGHTMEM_CONFIG_DIR='{shell_registry}' lmem --connection {id} --scope {scope} "
        );
        let key = platform.unwrap_or(&connection.context.harness_id);
        let registered = connection.context.harness_id.as_str();
        anyhow::ensure!(key == registered || Self::resolve_target(key, workspace).zip(Self::resolve_target(registered, workspace)).is_some_and(|(a,b)| a.0 == b.0), "Target harness must match the registered connection; register another connection for a different harness");
        anyhow::ensure!(key != "all", "A bound connection targets one harness; register separate connections for each harness");
        let (name, target) = if mcp {
            anyhow::ensure!(custom_path.is_none(), "--path selects a skill target; configure custom MCP clients using lmem --connection <id> mcp");
            Self::resolve_mcp_target(key, workspace).context("Automatic MCP configuration is unavailable for this harness; configure lmem --connection <id> mcp manually")?
        } else if let Some(path) = custom_path.or(connection.skill_path.as_deref()) {
            let target = if path.extension().is_some() {
                path.to_path_buf()
            } else {
                path.join("lightmem/SKILL.md")
            };
            let content = Self::bound_skill_content(connection, scope, &prefix);
            let (created, message) = Self::write_skill_content(&target, &content)?;
            return Ok(vec![ConnectResult {
                platform: key.into(),
                target_file: target,
                created,
                message,
            }]);
        } else {
            Self::resolve_target(key, workspace)
                .context("Custom harness needs a registered skill path or --path")?
        };
        let (created, message) = if mcp {
            Self::write_mcp_config_bound(
                &target,
                Some(vec![
                    "--connection".into(),
                    id.clone(),
                    "--scope".into(),
                    scope.into(),
                    "mcp".into(),
                ]),
                Some(&connection.registry_dir),
            )?
        } else {
            let content = Self::bound_skill_content(connection, scope, &prefix);
            Self::write_skill_content(&target, &content)?
        };
        Ok(vec![ConnectResult {
            platform: name.into(),
            target_file: target,
            created,
            message,
        }])
    }
    fn bound_skill_content(
        connection: &crate::models::ResolvedConnection,
        scope: &str,
        prefix: &str,
    ) -> String {
        let context = serde_json::to_string_pretty(connection).expect("Connection is serializable");
        format!(
            r#"---
name: lightmem
description: Persistent memory for this registered user, agent, and harness connection.
---

# LightMem connection

This harness is bound to the following identity and default scope:

```json
{context}
```

Use the command prefix below for every memory operation. It selects the registry and connection regardless of the current working directory. Do not combine it with `--db`, `LIGHTMEM_DB`, `-g`, or `--global`.

```bash
{prefix}context --json
{prefix}recall "relevant project decisions" --json
{prefix}remember "Decision: use SQLite WAL" -t decision --json
{prefix}remember "A verified finding" -t learning --confidence 0.95 --provenance validated --json
{prefix}remember "Current checkpoint" -t context --ttl 24h --json
{prefix}remember "Architecture diagram" -f ./diagram.png --json
{prefix}list -t decision --json
{prefix}inspect <memory-id> --json
{prefix}answer "What database did we choose?" --json
{prefix}observe transcript.txt --dry-run --json
{prefix}link <source-id> <target-id> -r depends_on --json
{prefix}related <memory-id> --hops 2 --json
{prefix}graph --json
{prefix}stats --json
```

Before working, recall relevant decisions, instructions, and context. Save atomic verified facts, decisions, preferences, and useful handoff notes with `remember`. `observe` extracts candidates from supplied text; installation does not automatically record conversations. Pass `--session <current-session-id>` when the harness provides one, and use a new ID for a new conversation.

The default scope is `{scope}`. Each command reads or writes one vault. To access personal preferences replace `--scope {scope}` with `--scope user`. Use `--scope project` for shared decisions (requires a registered project) and `--scope agent` for dedicated agent notes. Query another scope explicitly when needed. Memories retain their source identity through JSON export/import; deduplication retains the original source. Local identities organize memory and do not provide authentication.
"#
        )
    }

    const KNOWN_PLATFORMS: &'static [&'static str] = &[
        "antigravity",
        "codex",
        "hermes",
        "cursor",
        "claude",
        "agents",
    ];

    pub fn resolve_target(platform_key: &str, workspace: bool) -> Option<(&'static str, PathBuf)> {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let key_lower = platform_key.trim().to_lowercase();
        match key_lower.as_str() {
            "antigravity" | "gemini" | "agy" => {
                let base = if workspace {
                    PathBuf::from(".gemini/skills")
                } else {
                    let config_skills = home.join(".gemini/config/skills");
                    if config_skills.exists() {
                        config_skills
                    } else {
                        home.join(".gemini/skills")
                    }
                };
                Some(("Antigravity", base.join("lightmem/SKILL.md")))
            }
            "codex" | "openai" => {
                let base = if workspace {
                    PathBuf::from(".codex/skills")
                } else {
                    home.join(".codex/skills")
                };
                Some(("OpenAI Codex", base.join("lightmem/SKILL.md")))
            }
            "hermes" => {
                let base = if workspace {
                    PathBuf::from(".hermes/skills")
                } else {
                    let prod = home.join(".hermes/skills/productivity");
                    if prod.exists() {
                        prod
                    } else {
                        home.join(".hermes/skills")
                    }
                };
                Some(("Hermes Agent", base.join("lightmem/SKILL.md")))
            }
            "cursor" => {
                let base = if workspace {
                    PathBuf::from(".cursor/rules")
                } else {
                    home.join(".cursor/rules")
                };
                Some(("Cursor", base.join("lightmem.mdc")))
            }
            "claude" | "anthropic" => {
                let base = if workspace {
                    PathBuf::from(".claude/skills")
                } else {
                    home.join(".claude/skills")
                };
                Some(("Claude", base.join("lightmem/SKILL.md")))
            }
            "agents" | "agent" | "universal" => {
                let base = if workspace {
                    PathBuf::from(".agents/skills")
                } else {
                    home.join(".agents/skills")
                };
                Some(("Universal Agents", base.join("lightmem/SKILL.md")))
            }
            _ => None,
        }
    }

    pub fn is_platform_detected(platform_key: &str) -> bool {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let key_lower = platform_key.trim().to_lowercase();
        match key_lower.as_str() {
            "antigravity" | "gemini" | "agy" => {
                home.join(".gemini").exists() || Path::new(".gemini").exists()
            }
            "codex" | "openai" => home.join(".codex").exists() || Path::new(".codex").exists(),
            "hermes" => home.join(".hermes").exists() || Path::new(".hermes").exists(),
            "cursor" => home.join(".cursor").exists() || Path::new(".cursor").exists(),
            "claude" | "anthropic" => {
                home.join(".claude").exists()
                    || home.join(".claude.json").exists()
                    || Path::new(".claude").exists()
            }
            "agents" | "agent" | "universal" => {
                home.join(".agents").exists()
                    || home.join(".agent").exists()
                    || Path::new(".agents").exists()
                    || Path::new(".agent").exists()
            }
            _ => false,
        }
    }

    pub fn list_platforms(workspace: bool) -> Vec<PlatformInfo> {
        Self::KNOWN_PLATFORMS
            .iter()
            .filter_map(|&key| {
                Self::resolve_target(key, workspace).map(|(name, target)| PlatformInfo {
                    id: key.to_string(),
                    name: name.to_string(),
                    detected: Self::is_platform_detected(key),
                    target_file: target,
                })
            })
            .collect()
    }

    pub fn connect(
        platform: Option<&str>,
        workspace: bool,
        custom_path: Option<&Path>,
    ) -> Result<Vec<ConnectResult>> {
        let mut results = Vec::new();

        // 1. Custom path explicitly provided
        if let Some(p) = custom_path {
            let target_file = if p.extension().is_some() {
                p.to_path_buf()
            } else {
                p.join("lightmem/SKILL.md")
            };
            let (created, msg) = Self::write_skill_file(&target_file)?;
            results.push(ConnectResult {
                platform: "custom".to_string(),
                target_file,
                created,
                message: msg,
            });
            return Ok(results);
        }

        // 2. Specific platform requested or "all"
        let requested = platform.map(|s| s.trim().to_lowercase());

        let target_keys: Vec<&str> = match requested.as_deref() {
            Some("all") => Self::KNOWN_PLATFORMS.to_vec(),
            Some(single) => {
                if Self::resolve_target(single, workspace).is_some() {
                    vec![platform.unwrap()]
                } else {
                    anyhow::bail!(
                        "Unknown agent platform '{}'. Available platforms: {}, all",
                        single,
                        Self::KNOWN_PLATFORMS.join(", ")
                    );
                }
            }
            None => {
                // Auto-detect installed platforms
                let detected: Vec<&str> = Self::KNOWN_PLATFORMS
                    .iter()
                    .copied()
                    .filter(|&k| Self::is_platform_detected(k))
                    .collect();

                if detected.is_empty() {
                    // Default to Antigravity and Universal Agents
                    vec!["antigravity", "agents"]
                } else {
                    detected
                }
            }
        };

        for key in target_keys {
            if let Some((name, target_file)) = Self::resolve_target(key, workspace) {
                let (created, msg) = Self::write_skill_file(&target_file)?;
                results.push(ConnectResult {
                    platform: name.to_string(),
                    target_file,
                    created,
                    message: msg,
                });
            }
        }

        Ok(results)
    }

    fn write_skill_file(target: &Path) -> Result<(bool, String)> {
        Self::write_skill_content(target, SKILL_MD_CONTENT)
    }

    fn write_skill_content(target: &Path, content: &str) -> Result<(bool, String)> {
        let is_new = !target.exists();
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {:?}", parent))?;
        }

        std::fs::write(target, content)
            .with_context(|| format!("Failed to write skill file to {:?}", target))?;

        let msg = if is_new {
            "Installed new skill".to_string()
        } else {
            "Updated existing skill".to_string()
        };

        Ok((is_new, msg))
    }

    pub fn resolve_mcp_target(
        platform_key: &str,
        workspace: bool,
    ) -> Option<(&'static str, PathBuf)> {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let key_lower = platform_key.trim().to_lowercase();
        match key_lower.as_str() {
            "antigravity" | "gemini" | "agy" => {
                let base = if workspace {
                    PathBuf::from(".gemini/mcp_config.json")
                } else {
                    home.join(".gemini/config/mcp_config.json")
                };
                Some(("Antigravity", base))
            }
            "claude" | "anthropic" => {
                let path = if cfg!(target_os = "macos") {
                    home.join("Library/Application Support/Claude/claude_desktop_config.json")
                } else if cfg!(target_os = "windows") {
                    if let Ok(appdata) = std::env::var("APPDATA") {
                        PathBuf::from(appdata).join("Claude/claude_desktop_config.json")
                    } else {
                        home.join("AppData/Roaming/Claude/claude_desktop_config.json")
                    }
                } else {
                    home.join(".config/Claude/claude_desktop_config.json")
                };
                Some(("Claude Desktop", path))
            }
            "cursor" => {
                let path = if workspace {
                    PathBuf::from(".cursor/mcp.json")
                } else {
                    home.join(".cursor/mcp.json")
                };
                Some(("Cursor", path))
            }
            _ => None,
        }
    }

    pub fn connect_mcp(platform: Option<&str>, workspace: bool) -> Result<Vec<ConnectResult>> {
        let mut results = Vec::new();
        let target_keys: Vec<&str> = match platform {
            Some("all") => vec!["antigravity", "claude", "cursor"],
            Some(single) => {
                if Self::resolve_mcp_target(single, workspace).is_some() {
                    vec![single]
                } else {
                    anyhow::bail!(
                        "Unknown MCP platform '{}'. Available platforms: antigravity, claude, cursor, all",
                        single
                    );
                }
            }
            None => {
                let mut detected = Vec::new();
                if Self::is_platform_detected("antigravity") {
                    detected.push("antigravity");
                }
                if Self::is_platform_detected("claude") {
                    detected.push("claude");
                }
                if Self::is_platform_detected("cursor") {
                    detected.push("cursor");
                }
                if detected.is_empty() {
                    vec!["antigravity", "claude"]
                } else {
                    detected
                }
            }
        };

        for key in target_keys {
            if let Some((name, target_file)) = Self::resolve_mcp_target(key, workspace) {
                let (created, msg) = Self::write_mcp_config_file(&target_file)?;
                results.push(ConnectResult {
                    platform: name.to_string(),
                    target_file,
                    created,
                    message: msg,
                });
            }
        }

        Ok(results)
    }

    fn write_mcp_config_file(target: &Path) -> Result<(bool, String)> {
        Self::write_mcp_config_bound(target, None, None)
    }

    fn write_mcp_config_bound(
        target: &Path,
        args: Option<Vec<String>>,
        registry_dir: Option<&Path>,
    ) -> Result<(bool, String)> {
        let is_new = !target.exists();
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {:?}", parent))?;
        }

        let mut root: serde_json::Value = if target.exists() {
            let content = std::fs::read_to_string(target).unwrap_or_default();
            serde_json::from_str(&content)
                .context("Existing MCP config is invalid JSON; refusing to overwrite it")?
        } else {
            serde_json::json!({})
        };

        anyhow::ensure!(
            root.is_object(),
            "Existing MCP config must be a JSON object; refusing to overwrite it"
        );

        let obj = root.as_object_mut().unwrap();
        anyhow::ensure!(
            !obj.contains_key("mcpServers") || obj["mcpServers"].is_object(),
            "Existing mcpServers must be an object; refusing to overwrite it"
        );
        if !obj.contains_key("mcpServers") {
            obj.insert("mcpServers".to_string(), serde_json::json!({}));
        }

        let mcp_servers = obj.get_mut("mcpServers").unwrap().as_object_mut().unwrap();
        mcp_servers.insert(
            "lightmem".to_string(),
            serde_json::json!({
                "command": "lmem",
                "args": args.unwrap_or_else(|| vec!["mcp".into()])
            }),
        );

        if let Some(dir) = registry_dir {
            mcp_servers
                .get_mut("lightmem")
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert(
                    "env".into(),
                    serde_json::json!({"LIGHTMEM_CONFIG_DIR": dir}),
                );
        }
        let formatted = serde_json::to_string_pretty(&root)?;
        std::fs::write(target, formatted)
            .with_context(|| format!("Failed to write MCP config to {:?}", target))?;

        let msg = if is_new {
            "Created new MCP config with LightMem".to_string()
        } else {
            "Updated MCP config with LightMem".to_string()
        };

        Ok((is_new, msg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_skill_content_is_valid() {
        assert!(!SKILL_MD_CONTENT.is_empty());
        assert!(SKILL_MD_CONTENT.contains("name: lightmem"));
        assert!(SKILL_MD_CONTENT.contains("lmem remember"));
        assert!(SKILL_MD_CONTENT.contains("lmem recall"));
        assert!(SKILL_MD_CONTENT.contains("lmem answer"));
        assert!(SKILL_MD_CONTENT.contains("lmem graph"));
        assert!(SKILL_MD_CONTENT.contains("lmem link"));
        assert!(SKILL_MD_CONTENT.contains("lmem related"));
    }

    #[test]
    fn test_connect_to_custom_path() {
        let dir = tempfile::tempdir().unwrap();
        let results = ConnectService::connect(None, false, Some(dir.path())).unwrap();

        assert_eq!(results.len(), 1);
        let target = dir.path().join("lightmem/SKILL.md");
        assert!(target.exists());
        let read_back = std::fs::read_to_string(&target).unwrap();
        assert_eq!(read_back, SKILL_MD_CONTENT);
    }

    #[test]
    fn test_platform_resolution() {
        assert!(ConnectService::resolve_target("antigravity", false).is_some());
        assert!(ConnectService::resolve_target("codex", false).is_some());
        assert!(ConnectService::resolve_target("hermes", false).is_some());
        assert!(ConnectService::resolve_target("cursor", false).is_some());
        assert!(ConnectService::resolve_target("claude", false).is_some());
        assert!(ConnectService::resolve_target("agents", false).is_some());
        assert!(ConnectService::resolve_target("unknown-agent", false).is_none());
    }
}
