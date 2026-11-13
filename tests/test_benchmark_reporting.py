"""Source contracts for benchmark labels and measurement provenance."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]

class BenchmarkReportContracts(unittest.TestCase):
    def test_no_unevaluated_success_or_machine_claims(self):
        text = (ROOT / 'benches/bench_harness.rs').read_text()
        for claim in ['✅ **PASS**', 'Apple Silicon (macOS)', 'Rust 1.8x',
                      '0 Lock Errors', '0.0% (Zero Corruption)',
                      'without memory leaks', '✅ PERFECT', '0 BYTES CORRUPTED']:
            self.assertNotIn(claim, text)

    def test_recall_labels_do_not_claim_ground_truth(self):
        text = (ROOT / 'benches/bench_harness.rs').read_text()
        self.assertNotIn('hit_rate', text)
        self.assertNotIn('Hybrid Recall Latency & Accuracy', text)
        self.assertIn('Nonempty-result rate', text)
        self.assertIn('not retrieval accuracy', text)

    def test_timing_boundaries_are_disclosed(self):
        text = (ROOT / 'benches/bench_harness.rs').read_text()
        for required in ['in-process', 'no dedicated warm-up', 'CLI startup',
                         'not measured', 'std::env::consts::OS', 'b4.errors',
                         'b4.integrity_ok', 'b6.categories_intact']:
            self.assertIn(required, text)

if __name__ == '__main__':
    unittest.main()
