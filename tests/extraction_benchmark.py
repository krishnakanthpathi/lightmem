"""
LightMem Extractive QA Model Benchmark Runner
Tests span extraction accuracy, latency, exact match, token F1, and hallucination rejection
across TinyBERT, MobileBERT, MiniLM, DistilBERT, and ModernBERT-base.
"""

import subprocess
import time
import json
import os
import sys

BENCHMARK_QUESTIONS = [
    {
        "id": "Q1_friend_name",
        "category": "Personal Fact",
        "question": "what is King Grey's friend Bijo's full name",
        "ground_truth": "Datla Siva Ramaraju",
        "is_negative": False
    },
    {
        "id": "Q2_pan_number",
        "category": "Identifier / Tax",
        "question": "what is my PAN number",
        "ground_truth": "HAQPP8118D",
        "is_negative": False
    },
    {
        "id": "Q3_phone_number",
        "category": "Contact / Phone",
        "question": "what is my phone number",
        "ground_truth": "6281165645",
        "is_negative": False
    },
    {
        "id": "Q4_email_address",
        "category": "Contact / Email",
        "question": "what is my primary email address",
        "ground_truth": "krishnakanthpathi@gmail.com",
        "is_negative": False
    },
    {
        "id": "Q5_service_port",
        "category": "Infrastructure / Port",
        "question": "what port does memanto backend run on",
        "ground_truth": "7777",
        "is_negative": False
    },
    {
        "id": "Q6_mitv_ip",
        "category": "Network / IP",
        "question": "what is the IP address of Xiaomi Mi TV",
        "ground_truth": "192.168.31.184",
        "is_negative": False
    },
    {
        "id": "Q7_macbook_ram",
        "category": "Hardware / Specs",
        "question": "how much unified memory does the MacBook Air M4 have",
        "ground_truth": "16 GB",
        "is_negative": False
    },
    {
        "id": "Q8_badminton_time",
        "category": "Routine / Timetable",
        "question": "when is the fixed badminton session",
        "ground_truth": "18:00 to 19:00",
        "is_negative": False
    },
    {
        "id": "Q9_codeforces_handle",
        "category": "Profile / Handle",
        "question": "what is the user's Codeforces handle",
        "ground_truth": "krishnakanthpathi",
        "is_negative": False
    },
    {
        "id": "Q10_legal_name",
        "category": "Identity / Legal",
        "question": "what is the user's full legal name",
        "ground_truth": "Pathi Krishna Kanth",
        "is_negative": False
    },
    {
        "id": "Q11_shopping_list",
        "category": "Commitment / List",
        "question": "what items are on the Big Billion Days shopping checklist",
        "ground_truth": "ceiling fan, mosquito bat, and badminton racket grip",
        "is_negative": False
    },
    {
        "id": "Q12_tech_stack",
        "category": "Engineering / Stack",
        "question": "what language targets the location tracker project stack",
        "ground_truth": "Java",
        "is_negative": False
    },
    {
        "id": "Q13_neg_passport",
        "category": "Negative / Unanswerable",
        "question": "what is the user's passport number",
        "ground_truth": "INSUFFICIENT_EVIDENCE",
        "is_negative": True
    },
    {
        "id": "Q14_neg_flight",
        "category": "Negative / Unanswerable",
        "question": "what time is my flight to New York",
        "ground_truth": "INSUFFICIENT_EVIDENCE",
        "is_negative": True
    }
]

def normalize_text(s):
    if not s:
        return ""
    import re, string
    s = s.lower()
    s = "".join(ch for ch in s if ch not in set(string.punctuation))
    s = re.sub(r"\b(a|an|the)\b", " ", s)
    return " ".join(s.split())

def compute_f1(pred, gold):
    pred_tokens = normalize_text(pred).split()
    gold_tokens = normalize_text(gold).split()
    if not pred_tokens or not gold_tokens:
        return 1.0 if pred_tokens == gold_tokens else 0.0
    common = set(pred_tokens) & set(gold_tokens)
    if not common:
        return 0.0
    prec = sum(min(pred_tokens.count(w), gold_tokens.count(w)) for w in common) / len(pred_tokens)
    rec = sum(min(pred_tokens.count(w), gold_tokens.count(w)) for w in common) / len(gold_tokens)
    if prec + rec == 0:
        return 0.0
    return 2 * (prec * rec) / (prec + rec)

def run_single(model_name, q_item):
    base_model_dir = os.path.expanduser(f"~/.lightmem/models/qa/eval_benchmark/{model_name}")
    cmd = [
        "lmem", "answer", q_item["question"],
        "-r", f"onnx:{base_model_dir}",
        "--json"
    ]
    t0 = time.perf_counter()
    proc = subprocess.run(cmd, capture_output=True, text=True)
    dt_ms = (time.perf_counter() - t0) * 1000.0

    ans = ""
    conf = 0.0
    raw = proc.stdout
    try:
        data = json.loads(proc.stdout)
        ans = data.get("answer", "")
        conf = data.get("confidence", 0.0)
    except Exception as e:
        ans = f"PARSE_ERROR: {e}"

    is_no_evidence = ("insufficient evidence" in ans.lower() or not ans or ans == "PARSE_ERROR")

    if q_item["is_negative"]:
        em = 1.0 if is_no_evidence else 0.0
        f1 = 1.0 if is_no_evidence else 0.0
        hallucinated = not is_no_evidence
    else:
        norm_ans = normalize_text(ans)
        norm_gt = normalize_text(q_item["ground_truth"])
        em = 1.0 if (norm_ans == norm_gt or norm_gt in norm_ans) else 0.0
        f1 = compute_f1(ans, q_item["ground_truth"])
        hallucinated = False

    return {
        "id": q_item["id"],
        "category": q_item["category"],
        "question": q_item["question"],
        "ground_truth": q_item["ground_truth"],
        "is_negative": q_item["is_negative"],
        "predicted_answer": ans,
        "confidence": conf,
        "latency_ms": round(dt_ms, 2),
        "exact_match": em,
        "f1": round(f1, 4),
        "hallucinated": hallucinated
    }

def run_benchmark(target_models=None):
    if not target_models:
        target_models = ["tinybert", "mobilebert", "minilm", "distilbert", "modernbert"]
    
    results = {}
    for m in target_models:
        print(f"[*] Running benchmark for {m}...")
        model_results = []
        for q in BENCHMARK_QUESTIONS:
            res = run_single(m, q)
            model_results.append(res)
            print(f"    [{q['id']}] Ans: '{res['predicted_answer'][:40]}' | EM: {res['exact_match']} | Latency: {res['latency_ms']}ms")
        
        pos_res = [r for r in model_results if not r["is_negative"]]
        neg_res = [r for r in model_results if r["is_negative"]]

        avg_latency = sum(r["latency_ms"] for r in model_results) / len(model_results)
        pos_em = sum(r["exact_match"] for r in pos_res) / len(pos_res)
        pos_f1 = sum(r["f1"] for r in pos_res) / len(pos_res)
        neg_rejection_rate = sum(r["exact_match"] for r in neg_res) / len(neg_res) if neg_res else 1.0
        hallucination_rate = sum(1 for r in neg_res if r["hallucinated"]) / len(neg_res) if neg_res else 0.0

        results[m] = {
            "summary": {
                "positive_em": round(pos_em * 100, 2),
                "positive_f1": round(pos_f1 * 100, 2),
                "negative_rejection_accuracy": round(neg_rejection_rate * 100, 2),
                "hallucination_rate": round(hallucination_rate * 100, 2),
                "avg_latency_ms": round(avg_latency, 2),
            },
            "details": model_results
        }
    
    return results

if __name__ == "__main__":
    targets = sys.argv[1:] if len(sys.argv) > 1 else None
    res = run_benchmark(targets)
    out_path = "/tmp/extraction_benchmark_results.json"
    if targets and len(targets) == 1:
        out_path = f"/tmp/extraction_benchmark_{targets[0]}.json"
    with open(out_path, "w") as f:
        json.dump(res, f, indent=2)
    print(f"\n[+] Benchmark finished! Results written to {out_path}")
