#!/usr/bin/env python3
"""Needle 3 Precision Reranker & Slot Extractor for LightMem."""

import json
import os
import re
import sys

os.environ["NEEDLE_TELEMETRY"] = "0"

try:
    import needle
except ImportError:
    venv_site = "/Users/krishnakanth/Projects/needle3-mac-bench/.venv/lib/python3.11/site-packages"
    if os.path.exists(venv_site):
        sys.path.insert(0, venv_site)
    try:
        import needle
    except ImportError:
        print(json.dumps({"error": "needle package not installed"}))
        sys.exit(1)


def select_memory(memory_id: str, answer: str = ""):
    """Select candidate memory and provide extracted factual slot answer."""
    return {"memory_id": memory_id, "answer": answer}


def tokenize(text: str) -> set:
    words = re.findall(r"\w+", text.lower())
    stop_words = {"what", "is", "our", "the", "a", "an", "on", "in", "to", "for", "with", "does", "do", "how", "why"}
    return {w for w in words if w not in stop_words and len(w) > 1}


def extract_memory(title: str, content: str, category: str, tags: list):
    """Extract standard LightMem card slots.
    category must be one of: fact, decision, instruction, preference, learning, goal, commitment, artifact, event, relationship, observation, error, context, password.
    content is the main factual statement or secret value.
    title is a concise 2-6 word headline.
    tags is a list of relevant lowercase keyword strings.
    """
    return {"title": title, "content": content, "category": category, "tags": tags}


VALID_CATEGORIES = {
    "fact", "decision", "instruction", "preference", "learning", "goal",
    "commitment", "artifact", "event", "relationship", "observation",
    "error", "context", "password"
}

CATEGORY_SYNONYMS = {
    "credential": "password",
    "credentials": "password",
    "secret": "password",
    "token": "password",
    "key": "password",
    "api_key": "password",
    "passwords": "password",
    "rule": "instruction",
    "procedure": "instruction",
    "runbook": "instruction",
    "guideline": "instruction",
    "construction": "instruction",
    "decision": "decision",
    "decisions": "decision",
    "choice": "decision",
    "pref": "preference",
    "preferences": "preference",
    "like": "preference",
    "lesson": "learning",
    "learnings": "learning",
    "insight": "learning",
    "target": "goal",
    "goals": "goal",
    "objective": "goal",
    "todo": "commitment",
    "task": "commitment",
    "promise": "commitment",
    "code": "artifact",
    "doc": "artifact",
    "document": "artifact",
    "incident": "event",
    "meeting": "event",
    "bug": "error",
    "issue": "error",
    "exception": "error",
    "failure": "error",
    "background": "context",
}

def normalize_category(cat: str) -> str:
    if not cat:
        return "fact"
    c = cat.strip().lower()
    if c in VALID_CATEGORIES:
        return c
    return CATEGORY_SYNONYMS.get(c, "fact")

def handle_extract(data):
    objects = data.get("objects", [])
    if not objects:
        print(json.dumps({"results": []}))
        return

    extracted_results = []
    try:
        agent = needle.Needle(generation=3, tools=[extract_memory], stateless=True)
    except Exception as e:
        agent = None

    for obj in objects:
        raw_str = obj if isinstance(obj, str) else json.dumps(obj)
        if not agent:
            extracted_results.append({
                "title": raw_str[:40],
                "content": raw_str,
                "category": "fact",
                "tags": []
            })
            continue

        prompt = (
            f"Analyze this raw data object and extract a standardized LightMem memory:\n"
            f"Raw Data: {raw_str}\n\n"
            f"Call extract_memory with the extracted fields. category must strictly be one of: "
            f"fact, decision, instruction, preference, learning, goal, commitment, artifact, event, relationship, observation, error, context, password."
        )
        try:
            res = agent.run(prompt, strict=False)
            results = res.get("results", [])
            valid_item = None
            for item in results:
                if isinstance(item, dict) and item.get("content"):
                    item["category"] = normalize_category(item.get("category", "fact"))
                    valid_item = item
                    break
            if valid_item:
                extracted_results.append(valid_item)
            else:
                extracted_results.append({
                    "title": raw_str[:40],
                    "content": raw_str,
                    "category": "fact",
                    "tags": []
                })
        except Exception:
            extracted_results.append({
                "title": raw_str[:40],
                "content": raw_str,
                "category": "fact",
                "tags": []
            })

    print(json.dumps({"results": extracted_results}))


def main():
    if len(sys.argv) > 1 and sys.argv[1].strip():
        raw_input = sys.argv[1].strip()
    else:
        raw_input = sys.stdin.read().strip()
    if not raw_input:
        sys.exit(0)

    try:
        data = json.loads(raw_input)
    except Exception as e:
        print(json.dumps({"error": f"Invalid JSON: {e}"}))
        sys.exit(1)

    if data.get("action") == "extract" or "objects" in data:
        handle_extract(data)
        return

    question = data.get("question", "").strip()
    candidates = data.get("candidates", [])

    if not candidates:
        print(json.dumps({"answer": "No candidate memories provided.", "selected_id": None}))
        sys.exit(0)

    # Format candidates list
    lines = [f"Question: {question}\n\nCandidate Memories:"]
    for c in candidates:
        cid = c.get("id", "")
        title = c.get("title", "")
        content = c.get("content", "")
        lines.append(f"- ID: {cid} | Title: {title} | Content: {content}")

    lines.append("\nCall select_memory for the matching memory with its memory_id and answer.")
    prompt = "\n".join(lines)

    try:
        agent = needle.Needle(generation=3, tools=[select_memory], stateless=True)
        res = agent.run(prompt, strict=False)

        results = res.get("results", [])
        reasoning = res.get("reasoning", "")
        q_tokens = tokenize(question)

        best_candidate = None
        best_answer = None
        highest_score = -1.0

        # Evaluate any returned slots from Needle
        for item in results:
            if not isinstance(item, dict):
                continue
            item_id = item.get("memory_id", "")
            item_ans = item.get("answer", "")

            # Match against candidates
            for c in candidates:
                c_id = c.get("id", "")
                c_text = f"{c.get('title', '')} {c.get('content', '')}"
                c_tokens = tokenize(c_text)

                score = len(q_tokens.intersection(c_tokens)) * 2.0
                if item_id and (c_id == item_id or item_id in c_id or c_id in item_id):
                    score += 5.0
                if item_ans and item_ans.lower() in c_text.lower():
                    score += 3.0

                if score > highest_score:
                    highest_score = score
                    best_candidate = c
                    clean_ans = item_ans.strip()
                    if clean_ans.lower() in ("memory_id", "answer", "result", "none", "null", "") or len(clean_ans) < 3:
                        best_answer = c.get("content")
                    else:
                        best_answer = clean_ans

        # Fallback if Needle didn't match cleanly
        if not best_candidate:
            # Score candidates directly against query and reasoning
            for c in candidates:
                c_text = f"{c.get('title', '')} {c.get('content', '')}"
                c_tokens = tokenize(c_text)
                score = len(q_tokens.intersection(c_tokens)) * 2.0
                if reasoning:
                    r_tokens = tokenize(reasoning)
                    score += len(r_tokens.intersection(c_tokens))

                if score > highest_score:
                    highest_score = score
                    best_candidate = c
                    best_answer = c.get("content")

        if not best_candidate:
            best_candidate = candidates[0]
            best_answer = best_candidate.get("content")

        conf = float(res.get("confidence", 0.92))

        print(json.dumps({
            "selected_id": best_candidate.get("id"),
            "answer": best_answer,
            "confidence": conf,
            "model": "needle-3",
        }))

    except Exception as e:
        first = candidates[0] if candidates else {}
        print(json.dumps({
            "error": str(e),
            "selected_id": first.get("id"),
            "answer": first.get("content"),
            "confidence": 0.85,
            "model": "needle-fallback",
        }))


if __name__ == "__main__":
    main()
