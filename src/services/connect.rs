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
        let is_new = !target.exists();
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {:?}", parent))?;
        }

        std::fs::write(target, SKILL_MD_CONTENT)
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
        let is_new = !target.exists();
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {:?}", parent))?;
        }

        let mut root: serde_json::Value = if target.exists() {
            let content = std::fs::read_to_string(target).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        if !root.is_object() {
            root = serde_json::json!({});
        }

        let obj = root.as_object_mut().unwrap();
        if !obj.contains_key("mcpServers") || !obj["mcpServers"].is_object() {
            obj.insert("mcpServers".to_string(), serde_json::json!({}));
        }

        let mcp_servers = obj.get_mut("mcpServers").unwrap().as_object_mut().unwrap();
        mcp_servers.insert(
            "lightmem".to_string(),
            serde_json::json!({
                "command": "lmem",
                "args": ["mcp"]
            }),
        );

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
