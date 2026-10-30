# ❖ Answering Guide (`lmem answer`)

Command-driven reference for pinpoint factual extraction and neural question answering in LightMem.

---

## 1. Syntax

```bash
lmem answer [OPTIONS] <QUESTION>
```

---

## 2. Default Answering & Entity Index

### Ask a Factual Question
```bash
lmem answer "what port does postgresql use?"
```

Output:
```
✦ Answer [reranker: entity_index] ▰▰▰▰▰▰▰▰▰▰ 95%

  ▸ 5433

  ╰─▸ Source: [fact] Postgres Port Update (4156db6c)
```
* **Tier 1 (Sub-ms)**: If recalled memories contain an entity matching question keywords, returns the exact slot value instantly via `entity_index`.
* **Tier 2 (Neural QA)**: If no direct entity matches, passes candidates to the configured reranker.

---

## 3. Reranker Backends (`-r, --reranker`)

### 1. Instant Top-1 Vector Answer (`-r top1`)
```bash
lmem answer "what did we decide for database?" -r top1
```
* Zero-overhead (0ms reranker latency).
* Returns the content of the highest-ranked vector memory directly.

### 2. Local ONNX Extractive QA (`-r minilm-squad2`)
```bash
lmem answer "what is my api key?" -r minilm-squad2
```
* 100% offline, local ONNX neural model (`deepset/minilm-uncased-squad2`).
* Extracts exact token span (under 20 tokens) with ~600ms latency.
* Zero cloud dependencies and zero API costs.

### 3. High-Accuracy Local ONNX (`-r tinyroberta-squad2`)
```bash
lmem answer "what is the root cause of the crash?" -r tinyroberta-squad2
```
* Local ONNX RoBERTa model trained on SQuAD-2.0.
* Higher precision on complex, multi-sentence contexts (~1.2s latency).

### 4. Swappable Ollama LLM Reranker (`-r ollama:<model>`)
```bash
# Use currently configured default Ollama model
lmem answer "what language was chosen for performance?" -r ollama

# Override with a specific local model
lmem answer "summarize sprint goals" -r ollama:qwen2.5:3b

# Override with an Ollama cloud model
lmem answer "what are our compliance requirements?" -r ollama:gemma4:31b-cloud
```
* Invokes LLM over standard Ollama HTTP API.
* Synthesizes and extracts answers across multi-paragraph candidates.

---

## 4. Candidate Retrieval Pool (`-l, --limit`)

### Control Candidate Memory Depth
```bash
# Retrieve top 5 candidate memories before reranking
lmem answer "auth port" -l 5

# Retrieve top 20 candidate memories for broad synthesis
lmem answer "incident timeline" --limit 20
```
* Default: 10 candidates.
* Larger pools improve recall for complex questions; smaller pools run faster.

---

## 5. Category Filtering (`-t, --type`)

### Restrict Candidates to a Specific Memory Category
```bash
# Query architectural choices only
lmem answer "which database do we use?" -t decision

# Query user preferences only
lmem answer "which editor theme is preferred?" -t preference

# Query workflow instructions only
lmem answer "what is the git commit rule?" -t instruction
```

Supported types: `fact`, `decision`, `instruction`, `preference`, `learning`, `goal`, `commitment`, `artifact`, `event`, `relationship`, `observation`, `error`, `context`, `password`.

---

## 6. Temporal Answering (`--as-of` vs `--date`)

### Historical Point-in-Time Answer (`--as-of`)
```bash
# What was the answer before the memory was superseded?
lmem answer "what is the postgres port?" --as-of 2026-09-15
lmem answer "what is the postgres port?" --as-of 2026-09-15T14:30:00Z
```
* Evaluates active vault state at that specific historical moment.
* Answers with older active facts that were later updated or expired.

### Calendar Day Filter (`--date`)
```bash
# Answer based strictly on memories logged on that date
lmem answer "what were the sprint updates?" --date 2026-10-30
```
* Format: `YYYY-MM-DD`. Restricts candidate memories to that exact UTC day.

---

## 7. Multi-Tenant Scopes & Connections

### Scoped Vault Answering
```bash
# Query inside a named connection vault
lmem --connection agy answer "what port does auth use?"

# Query inside an isolated agent scope
lmem --connection agy --scope agent answer "current debugging state"

# Query inside a user-wide scope
lmem --connection agy --scope user answer "user preferences"
```

---

## 8. Vault Overrides

### Global Vault
```bash
lmem -g answer "what is my preferred shell?"
```
* Forces query against `~/.lightmem/memories.db`, bypassing local repository vaults.

### Custom Database Path
```bash
lmem --db /path/to/custom.db answer "what is the encryption key?"
```

---

## 9. Machine-Readable JSON Output (`--json`)

```bash
lmem answer "what language was chosen for performance?" -r ollama --json
```

Output:
```json
{
  "answer": "Rust",
  "selected_memory": {
    "id": "b6eb28be-a45d-4e1b-909a-316742b9c191",
    "category": "decision",
    "title": "We decided to use Rust for performance",
    "content": "We decided to use Rust for performance",
    "tags": [],
    "confidence": 0.9,
    "status": "active",
    "provenance": "explicit_statement",
    "created_at": "2026-10-30T14:36:46.913105Z",
    "updated_at": "2026-10-30T14:36:46.913105Z",
    "expired_at": null,
    "entities": [
      { "name": "programming_language", "val": "Rust" }
    ]
  },
  "confidence": 1.0,
  "reranker_used": "ollama:gemma4:31b-cloud"
}
```

---

## 10. Python SDK Equivalents

```python
from lmem import LightMem

lm = LightMem()

# 1. Ask question (default entity_index + configured reranker)
res = lm.answer("what port does postgres use?")
print(f"Answer: {res.answer}")
print(f"Confidence: {res.confidence:.2f}")
print(f"Reranker: {res.reranker_used}")
print(f"Source ID: {res.selected_memory.id}")

# 2. Force zero-overhead vector top-1
top = lm.answer("what did we decide for database?", reranker="top1")

# 3. Filter by category
decision_answer = lm.answer("which database?", category="decision")

# 4. Point-in-time historical answer
historical = lm.answer("what is the port?", as_of="2026-09-15")

# 5. Scoped connection
scoped = LightMem(connection="agy")
agent_answer = scoped.answer("current task status")
```
