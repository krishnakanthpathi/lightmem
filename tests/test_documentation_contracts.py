"""Keep versioned installation and documentation entry points aligned."""
from html.parser import HTMLParser
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]


class Page(HTMLParser):
    def __init__(self, text):
        super().__init__()
        self.ids, self.links, self.assets = [], [], []
        self.feed(text)

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if "id" in attrs:
            self.ids.append(attrs["id"])
        if tag == "a" and "href" in attrs:
            self.links.append(attrs["href"])
        if tag == "script" and "src" in attrs:
            self.assets.append(attrs["src"])
        if tag == "link" and attrs.get("rel") in ("stylesheet", "icon"):
            self.assets.append(attrs["href"])


class DocumentationContracts(unittest.TestCase):
    def test_static_entry_points_and_internal_links_exist(self):
        for name in ["index.html", "docs.html", "workspace.html"]:
            page = Page((ROOT / "static" / name).read_text())
            self.assertEqual(len(page.ids), len(set(page.ids)), name)
            for link in page.links:
                if link.startswith("#") and name != "workspace.html":
                    self.assertIn(link[1:], page.ids, (name, link))
            for asset in page.assets:
                self.assertTrue((ROOT / "static" / asset.split("?", 1)[0].lstrip("/")).is_file(), (name, asset))

    def test_source_versions_and_release_defaults_match(self):
        version = re.search(r'^version = "([^"]+)"', (ROOT / "Cargo.toml").read_text(), re.M)[1]
        for file in ["python/pyproject.toml", "python/src/lmem/__init__.py", "static/index.html", "static/docs.html", "install.sh", "install.ps1"]:
            self.assertIn(version, (ROOT / file).read_text(), file)

    def test_first_memory_example_has_no_missing_attachment_dependency(self):
        docs = (ROOT / "static/docs.html").read_text()
        cycle = docs.split('id="first-memory"', 1)[1].split('</pre>', 1)[0]
        self.assertNotIn('./arch.png', cycle)
        self.assertNotIn(' -f ', cycle)
        for entry in ['id="command-ui"', 'id="command-serve"', 'id="install-windows"']:
            self.assertIn(entry, docs)


if __name__ == "__main__":
    unittest.main()
