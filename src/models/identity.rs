//! Local identity registry and deterministic, physically separated memory scopes.
use crate::models::LightMemConfig;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    User,
    Project,
    Agent,
}

impl std::str::FromStr for MemoryScope {
    type Err = String;
    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "user" => Ok(Self::User),
            "project" => Ok(Self::Project),
            "agent" => Ok(Self::Agent),
            _ => Err("Scope must be user, project, or agent".into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryContext {
    pub connection_id: String,
    pub user_id: String,
    pub agent_id: String,
    pub harness_id: String,
    pub project_id: Option<String>,
    pub scope: MemoryScope,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityEntry {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedConnection {
    #[serde(flatten)]
    pub context: MemoryContext,
    pub db_path: PathBuf,
    pub registry_dir: PathBuf,
    pub skill_path: Option<PathBuf>,
}

pub struct IdentityRegistry {
    conn: Connection,
    root: PathBuf,
}

pub fn validate_id(id: &str) -> Result<()> {
    anyhow::ensure!(
        !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
        "IDs must contain 1–128 ASCII letters, digits, underscores, or hyphens"
    );
    Ok(())
}

impl IdentityRegistry {
    pub fn open_default() -> Result<Self> {
        Self::open(LightMemConfig::config_dir())
    }

    pub fn open(root: PathBuf) -> Result<Self> {
        let root = if root.is_absolute() {
            root
        } else {
            std::env::current_dir()?.join(root)
        };
        std::fs::create_dir_all(&root)?;
        let path = root.join("identities.db");
        let conn = Connection::open(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS users(id TEXT PRIMARY KEY, name TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS agents(id TEXT PRIMARY KEY, name TEXT NOT NULL, user_id TEXT NOT NULL REFERENCES users(id));
            CREATE TABLE IF NOT EXISTS harnesses(id TEXT PRIMARY KEY, name TEXT NOT NULL, skill_path TEXT);
            CREATE TABLE IF NOT EXISTS connections(id TEXT PRIMARY KEY, context TEXT NOT NULL);")?;
        for id in [
            "antigravity",
            "codex",
            "hermes",
            "cursor",
            "claude",
            "agents",
        ] {
            conn.execute(
                "INSERT OR IGNORE INTO harnesses(id,name) VALUES (?1,?1)",
                [id],
            )?;
        }
        Ok(Self { conn, root })
    }

    pub fn add_user(&self, id: &str, name: Option<&str>) -> Result<IdentityEntry> {
        validate_id(id)?;
        self.conn
            .execute(
                "INSERT INTO users VALUES (?1,?2)",
                params![id, name.unwrap_or(id)],
            )
            .context("User already exists or could not be registered")?;
        Ok(IdentityEntry {
            id: id.into(),
            name: name.unwrap_or(id).into(),
            user_id: None,
            skill_path: None,
        })
    }

    pub fn add_agent(&self, id: &str, user: &str, name: Option<&str>) -> Result<IdentityEntry> {
        validate_id(id)?;
        validate_id(user)?;
        self.conn
            .execute(
                "INSERT INTO agents VALUES (?1,?2,?3)",
                params![id, name.unwrap_or(id), user],
            )
            .context("Agent already exists or its user is not registered")?;
        Ok(IdentityEntry {
            id: id.into(),
            name: name.unwrap_or(id).into(),
            user_id: Some(user.into()),
            skill_path: None,
        })
    }

    pub fn add_harness(
        &self,
        id: &str,
        path: Option<PathBuf>,
        name: Option<&str>,
    ) -> Result<IdentityEntry> {
        validate_id(id)?;
        let path = path
            .map(|p| {
                if p.is_absolute() {
                    Ok(p)
                } else {
                    Ok::<_, std::io::Error>(std::env::current_dir()?.join(p))
                }
            })
            .transpose()?;
        self.conn
            .execute(
                "INSERT INTO harnesses VALUES (?1,?2,?3)",
                params![
                    id,
                    name.unwrap_or(id),
                    path.as_ref().map(|p| p.to_string_lossy().into_owned())
                ],
            )
            .context("Harness already exists or could not be registered")?;
        Ok(IdentityEntry {
            id: id.into(),
            name: name.unwrap_or(id).into(),
            user_id: None,
            skill_path: path,
        })
    }

    pub fn add_connection(
        &self,
        id: &str,
        user: &str,
        agent: &str,
        harness: &str,
        project: Option<String>,
        scope: MemoryScope,
    ) -> Result<ResolvedConnection> {
        for value in [id, user, agent, harness] {
            validate_id(value)?;
        }
        if let Some(ref project) = project {
            validate_id(project)?;
        }
        let owner: Option<String> = self
            .conn
            .query_row("SELECT user_id FROM agents WHERE id=?1", [agent], |r| {
                r.get(0)
            })
            .optional()?;
        anyhow::ensure!(
            owner.as_deref() == Some(user),
            "Agent must be registered to the selected user"
        );
        let exists: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM harnesses WHERE id=?1)",
            [harness],
            |r| r.get(0),
        )?;
        anyhow::ensure!(exists, "Harness is not registered");
        let context = MemoryContext {
            connection_id: id.into(),
            user_id: user.into(),
            agent_id: agent.into(),
            harness_id: harness.into(),
            project_id: project,
            scope,
            session_id: None,
        };
        self.resolve_context(context.clone())?;
        self.conn
            .execute(
                "INSERT INTO connections VALUES (?1,?2)",
                params![id, serde_json::to_string(&context)?],
            )
            .context("Connection already exists or could not be registered")?;
        self.resolve_context(context)
    }

    pub fn resolve(
        &self,
        id: &str,
        scope: Option<MemoryScope>,
        session: Option<String>,
    ) -> Result<ResolvedConnection> {
        let raw: String = self
            .conn
            .query_row("SELECT context FROM connections WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .with_context(|| format!("Unknown connection '{id}'"))?;
        let mut context: MemoryContext = serde_json::from_str(&raw)?;
        if let Some(scope) = scope {
            context.scope = scope;
        }
        if let Some(ref session) = session {
            validate_id(session)?;
        }
        context.session_id = session;
        self.resolve_context(context)
    }

    fn resolve_context(&self, context: MemoryContext) -> Result<ResolvedConnection> {
        let component = |value: &str| crate::models::compute_sha256_hex(value.as_bytes());
        let base = self.root.join("vaults").join(component(&context.user_id));
        let db_path = match context.scope {
            MemoryScope::User => base.join("user.db"),
            MemoryScope::Project => base.join("projects").join(format!(
                "{}.db",
                component(
                    context
                        .project_id
                        .as_deref()
                        .context("Project scope requires a project ID")?
                )
            )),
            MemoryScope::Agent => {
                let base = base.join("agents").join(component(&context.agent_id));
                match &context.project_id {
                    Some(project) => base
                        .join("projects")
                        .join(format!("{}.db", component(project))),
                    None => base.join("agent.db"),
                }
            }
        };
        let path: Option<String> = self.conn.query_row(
            "SELECT skill_path FROM harnesses WHERE id=?1",
            [&context.harness_id],
            |r| r.get(0),
        )?;
        Ok(ResolvedConnection {
            registry_dir: self.root.clone(),
            context,
            db_path,
            skill_path: path.map(PathBuf::from),
        })
    }

    pub fn list(&self, kind: &str) -> Result<serde_json::Value> {
        if kind == "connections" {
            let mut stmt = self
                .conn
                .prepare("SELECT id FROM connections ORDER BY id")?;
            let ids: Vec<String> = stmt
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            return Ok(serde_json::to_value(
                ids.iter()
                    .map(|id| self.resolve(id, None, None))
                    .collect::<Result<Vec<_>>>()?,
            )?);
        }
        let sql = match kind {
            "users" => "SELECT id,name,NULL,NULL FROM users ORDER BY id",
            "agents" => "SELECT id,name,user_id,NULL FROM agents ORDER BY id",
            "harnesses" => "SELECT id,name,NULL,skill_path FROM harnesses ORDER BY id",
            _ => anyhow::bail!("Kind must be users, agents, harnesses, or connections"),
        };
        let mut stmt = self.conn.prepare(sql)?;
        let entries = stmt
            .query_map([], |r| {
                Ok(IdentityEntry {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    user_id: r.get(2)?,
                    skill_path: r.get::<_, Option<String>>(3)?.map(PathBuf::from),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(serde_json::to_value(entries)?)
    }
}
