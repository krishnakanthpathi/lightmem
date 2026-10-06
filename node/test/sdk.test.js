const test = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const path = require("node:path");
const os = require("node:os");
const { spawnSync } = require("node:child_process");

const {
  LightMem,
  getBinaryPath,
  isNativeBinary,
  VERSION,
} = require("../dist/index.js");

test("LightMem SDK - Package exports and version", () => {
  assert.strictEqual(VERSION, "0.2.4");
  assert.strictEqual(typeof LightMem, "function");
  assert.strictEqual(typeof getBinaryPath, "function");
  assert.strictEqual(typeof isNativeBinary, "function");
});

test("LightMem SDK - Binary resolution", () => {
  const binary = getBinaryPath();
  assert.ok(binary && binary.length > 0, "Binary path should be non-empty");
  assert.ok(fs.existsSync(binary), `Binary at ${binary} should exist on disk`);
  assert.ok(isNativeBinary(binary), `Binary at ${binary} should be a valid native binary`);
});

test("LightMem CLI - Executable bin/lmem.js", () => {
  const binPath = path.resolve(__dirname, "../bin/lmem.js");
  assert.ok(fs.existsSync(binPath), "CLI script bin/lmem.js must exist");

  const res = spawnSync(process.execPath, [binPath, "--version"], {
    encoding: "utf-8",
  });
  assert.strictEqual(res.status, 0, `CLI failed with: ${res.stderr}`);
  assert.match(res.stdout, /lmem 0\.2\.\d+/, "CLI should output version string");
});

test("LightMem SDK - Full Memory Lifecycle (Isolated Vault)", async (t) => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "lmem-test-node-"));
  const tmpDb = path.join(tmpDir, "vault_test.db");

  t.after(() => {
    try {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    } catch {}
  });

  const client = new LightMem({ dbPath: tmpDb });

  // 1. Initial Stats (Empty vault)
  const initialStats = await client.stats();
  assert.strictEqual(initialStats.total_memories, 0);

  // 2. Remember a fact
  const created = await client.remember("PostgreSQL primary node listens on port 5432", {
    category: "fact",
    title: "PostgreSQL Port",
    tags: ["database", "postgres", "infra"],
    confidence: 0.95,
  });

  assert.ok(created.id, "Memory must have an ID");
  assert.strictEqual(created.category, "fact");
  assert.strictEqual(created.title, "PostgreSQL Port");
  assert.match(created.content, /5432/);
  assert.strictEqual(created.tags.length, 3);

  // 3. Stats after remember
  const statsAfter = await client.stats();
  assert.strictEqual(statsAfter.total_memories, 1);
  assert.strictEqual(statsAfter.active_memories, 1);

  // 4. Recall (Hybrid Search)
  const recalled = await client.recall("postgres port", { limit: 5 });
  assert.ok(recalled.length >= 1, "Should recall at least 1 memory");
  assert.strictEqual(recalled[0].memory.id, created.id);
  assert.ok(recalled[0].score > 0, "Score should be positive");

  // 5. Answer (Extractive QA)
  const ans = await client.answer("What port does PostgreSQL use?", { limit: 5 });
  assert.ok(ans.answer && ans.answer.length > 0, "Answer should not be empty");
  assert.match(ans.answer, /5432/, "Extracted answer should contain port 5432");

  // 6. Inspect
  const inspected = await client.inspect(created.id);
  assert.strictEqual(inspected.memory.id, created.id);
  assert.ok(Array.isArray(inspected.links));

  // 7. List
  const listRes = await client.list({ limit: 10, page: 1 });
  assert.strictEqual(listRes.total, 1);
  assert.strictEqual(listRes.items.length, 1);
  assert.strictEqual(listRes.items[0].id, created.id);

  // 8. Forget
  const forgotten = await client.forget(created.id);
  assert.strictEqual(forgotten, true);

  const statsAfterForget = await client.stats();
  assert.strictEqual(statsAfterForget.active_memories, 0);
  assert.strictEqual(statsAfterForget.expired_memories, 1);
});

test("LightMem SDK - Synchronous API methods", (t) => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "lmem-test-sync-"));
  const tmpDb = path.join(tmpDir, "vault_sync.db");

  t.after(() => {
    try {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    } catch {}
  });

  const client = new LightMem({ dbPath: tmpDb });

  const mem = client.rememberSync("Redis runs on port 6379", {
    category: "fact",
    title: "Redis Port",
  });
  assert.ok(mem.id);

  const stats = client.statsSync();
  assert.strictEqual(stats.total_memories, 1);

  const recalled = client.recallSync("redis port");
  assert.ok(recalled.length >= 1);
  assert.strictEqual(recalled[0].memory.id, mem.id);
});
