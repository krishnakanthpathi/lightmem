from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parent.parent


class DocumentationContractsTests(unittest.TestCase):
    def test_removed_web_and_stale_doc_artifacts_are_absent(self) -> None:
        removed_paths = [
            "static",
            "vercel.json",
            "src/services/workspace.rs",
            "src/services/docs_server.rs",
            "artifacts",
            "dist",
            "slides.html",
            "build_lightmem_deck.py",
            "LightMem_Skill_Guide.pptx",
            "PRESENTATION_NOTES.md",
            "demo_vault",
            "docs/cli-review-0.8.0.md",
            "docs/review-0.8.0.md",
            "docs/interface-inventory-2026-11-14.md",
            "docs/project-audit-2026-11-14.md",
            "docs/terminal-experience-proposal.md",
            "docs/command-line-experience-proposal.md",
        ]
        for rel in removed_paths:
            self.assertFalse((ROOT / rel).exists(), f"stale artifact still present: {rel}")

    def test_active_cli_guides_and_skill_exist(self) -> None:
        required_docs = [
            "README.md",
            "CHANGELOG.md",
            "SKILL.md",
            "docs/answering.md",
            "docs/conflicts_and_deduplication.md",
            "docs/forgetting_and_clearing.md",
            "docs/inspecting_and_listing.md",
            "docs/recalling.md",
            "docs/remembering.md",
            "docs/terminal-commands.md",
        ]
        for rel in required_docs:
            self.assertTrue((ROOT / rel).is_file(), f"missing documentation file: {rel}")

    def test_release_version_is_consistent_across_manifests(self) -> None:
        cargo_toml = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
        cargo_match = re.search(r'^version\s*=\s*"([^"]+)"', cargo_toml, re.MULTILINE)
        self.assertIsNotNone(cargo_match)
        version = cargo_match.group(1)

        pyproject = (ROOT / "python" / "pyproject.toml").read_text(encoding="utf-8")
        py_init = (ROOT / "python" / "src" / "lmem" / "__init__.py").read_text(encoding="utf-8")
        py_bin = (ROOT / "python" / "src" / "lmem" / "binary.py").read_text(encoding="utf-8")
        install_sh = (ROOT / "install.sh").read_text(encoding="utf-8")
        install_ps1 = (ROOT / "install.ps1").read_text(encoding="utf-8")
        changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")

        self.assertRegex(
            pyproject,
            re.compile(rf'^version\s*=\s*"{re.escape(version)}"', re.MULTILINE),
        )
        self.assertIn(f'__version__ = "{version}"', py_init)
        self.assertIn(f'version: str = "v{version}"', py_bin)
        self.assertIn(f"VERSION=\"${{LIGHTMEM_VERSION:-v{version}}}\"", install_sh)
        self.assertIn(f'"v{version}"', install_ps1)
        self.assertIn(f"## {version}", changelog)


if __name__ == "__main__":
    unittest.main()
