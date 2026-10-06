# ❖ LightMem Node.js & TypeScript SDK (`lmem`)

**Ultra-Fast Local AI Agent Persistent Memory Engine with Neural Extractive QA & Knowledge Graph.**

Zero external runtime dependencies. Powered by the pure-Rust `lmem` native engine with SQLite WAL, FTS5 BM25, and local ONNX embedding & extractive QA models.

---

## ◈ Installation

```bash
# Install as a project dependency
npm install lmem
# or
pnpm add lmem
# or
yarn add lmem

# Or install globally for the CLI
npm install -g lmem
```

---

## ✦ Quickstart (TypeScript / JavaScript)

```typescript
import { LightMem } from "lmem";

// 1. Initialize client (defaults to global vault ~/.lightmem/memories.db)
const mem = new LightMem();

// Or use a custom project-specific SQLite vault:
// const mem = new LightMem({ dbPath: "./project_vault.db" });

async function main() {
  // 2. Remember a fact, decision, or credential (14 auto-inferred categories)
  const memory = await mem.remember(
    "PostgreSQL primary database runs on port 5432 with WAL archiving enabled",
    {
      category: "fact",
      title: "Postgres Config",
      tags: ["postgres", "database", "infra"],
    }
  );
  console.log(`Stored memory ID: ${memory.id}`);

  // 3. Hybrid Semantic + Keyword Recall
  const results = await mem.recall("what port does postgres use?", { limit: 5 });
  for (const { memory, score } of results) {
    console.log(`[${score.toFixed(2)}] ${memory.title}: ${memory.content}`);
  }

  // 4. Extractive Question Answering (via local ONNX SQuAD-2.0 or Ollama)
  const answer = await mem.answer("What is the postgres port?");
  console.log(`Answer: ${answer.answer} (Confidence: ${answer.confidence})`);

  // 5. Vault Telemetry & Stats
  const stats = await mem.stats();
  console.log(`Total memories: ${stats.total_memories}, Vectors: ${stats.vector_count}`);
}

main().catch(console.error);
```

### Synchronous API (for scripts & REPL)

Every asynchronous method has a zero-overhead synchronous counterpart:

```typescript
const memory = mem.rememberSync("Redis runs on port 6379");
const results = mem.recallSync("redis port");
const answer = mem.answerSync("What port does redis use?");
const stats = mem.statsSync();
```

---

## ⌖ API Reference

### `new LightMem(config?)`
* `dbPath?: string`: Path to specific SQLite file (defaults to `~/.lightmem/memories.db`).
* `globalDb?: boolean`: Force global database even if working in another directory.
* `binaryPath?: string`: Custom path to `lmem` executable.

### Core Methods

| Method | Description |
| :--- | :--- |
| `mem.remember(content, options?)` | Store a memory with category, title, tags, and optional TTL. |
| `mem.recall(query, options?)` | Hybrid BM25 + Vector search with Reciprocal Rank Fusion. |
| `mem.answer(question, options?)` | Precise extractive QA span extraction from retrieved evidence. |
| `mem.list(options?)` | Paginated chronological listing of active or expired memories. |
| `mem.get(id)` | Fetch single memory by ID or short prefix. |
| `mem.inspect(id)` | Detailed inspection including incoming and outgoing knowledge graph links. |
| `mem.forget(id, { hard? })` | Soft-expire or hard-delete a memory. |
| `mem.conflicts(options?)` | Detect contradictions, near-duplicates, and superseding candidates. |
| `mem.stats()` | Real-time vault storage telemetry and active backend stats. |
| `mem.link(sourceId, targetId, rel)` | Create a bidirectional knowledge graph wikilink between memories. |
| `mem.unlink(sourceId, targetId, rel)`| Remove a knowledge graph wikilink. |
| `mem.related(id, { hops? })` | Multi-hop graph traversal from a root memory. |
| `mem.graph()` | Knowledge graph network tree and adjacency matrix. |
| `mem.autolink()` | Automatically discover and link related memories across the vault. |
| `mem.reindex()` | Recompute vector embeddings and BM25 index. |

---

## ⌘ CLI Usage

When installed via npm, the `lmem` and `lightmem` commands are directly available:

```bash
# Run CLI directly
npx lmem stats
npx lmem remember "Auth secret key configured in AWS Secrets Manager" -t password
npx lmem recall "auth secret"
npx lmem answer "where is the auth secret stored?"
```

---

## ⚡ Architecture & Performance

* **Sub-10ms Cold Startup**: `getBinaryPath()` resolves the precompiled native engine directly, with zero Node.js VM boot overhead.
* **Zero Runtime Dependencies**: Uses built-in Node.js child processes and standard streams.
* **Automatic Binary Fallback**: If no bundled executable is present, automatically downloads the official release binary for the target architecture (`darwin-arm64`, `darwin-x64`, `linux-x64`, `win32-x64`) from GitHub Releases into `~/.lightmem/bin/`.

---

## License

MIT © [Pathi Krishna Kanth](https://github.com/krishnakanthpathi)
