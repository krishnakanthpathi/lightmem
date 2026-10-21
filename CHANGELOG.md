# Changelog

## 0.4.0

- Register stable users, agents, built-in or custom harnesses, and named connections.
- Select connections with `--connection` / `LIGHTMEM_CONNECTION` and inspect them with `context`.
- Share project decisions across harnesses using the same user/project; keep user and agent scopes in separate SQLite vaults. Agent scopes also separate projects.
- Record the originating connection, user, agent, harness, project, scope, and optional session on memories; preserve this metadata through backups.
- Generate bound skill and MCP configurations. Custom harness registrations can specify a persistent skill installation path.
- Add Python SDK registration methods and `LightMem(connection=..., scope=..., session=...)`, `context()`, and MCP setup support. MCP writes accept a per-call session ID.
- Verify wheel RECORD hashes after native retagging and fail downloads instead of falling back to an incompatible older engine.
- Gate both release workflows on Rust and Python tests and smoke test installed wheels.
- Migrate schema v4 to v5 transactionally. Existing memories retain unknown source identity and are not automatically reassigned to a registered user.
- Reject ambiguous database/connection options and preserve invalid MCP configuration files instead of replacing them.

Each command operates on one selected scope; recall does not combine user, project, and agent vaults. Identity scopes organize local memory and do not provide authentication between operating-system users. Conversation extraction still requires `observe` or a harness invocation; installing a connection does not record chats automatically.
