import re
import unittest
from pathlib import Path
from urllib.parse import unquote, urlsplit


ROOT = Path(__file__).resolve().parents[2]
DOCS = ROOT / "docs"
DOCUMENTS = sorted(path for path in DOCS.rglob("*.md") if "old" not in path.parts)


def prose(source):
    return re.sub(r"^```[^\n]*\n.*?^```\s*$", "", source, flags=re.M | re.S)


def links(path):
    return re.findall(r"\[[^\]\n]+\]\(([^)\s]+)\)", prose(path.read_text()))


def anchors(path):
    result = set()
    counts = {}
    for heading in re.findall(r"^#{1,6} (.+)$", prose(path.read_text()), re.M):
        slug = re.sub(r"[^\w\- ]", "", heading.lower()).replace(" ", "-")
        count = counts.get(slug, 0)
        counts[slug] = count + 1
        result.add(f"{slug}-{count}" if count else slug)
    return result


class DocumentationTests(unittest.TestCase):
    def test_local_links_and_heading_anchors(self):
        for path in [ROOT / "README.md", *DOCUMENTS]:
            for link in links(path):
                with self.subTest(page=path.relative_to(ROOT), link=link):
                    url = urlsplit(link)
                    if url.scheme or url.netloc:
                        continue
                    target = (
                        (path.parent / unquote(url.path)).resolve()
                        if url.path
                        else path
                    )
                    self.assertTrue(target.exists(), f"Missing target: {target}")
                    if (
                        url.fragment
                        and target.suffix == ".md"
                        and DOCS / "old" not in target.parents
                    ):
                        self.assertIn(unquote(url.fragment), anchors(target))

    def test_all_current_pages_are_reachable_from_documentation_index(self):
        pending = [DOCS / "README.md"]
        visited = set()
        while pending:
            path = pending.pop()
            if path in visited:
                continue
            visited.add(path)
            for link in links(path):
                url = urlsplit(link)
                if url.scheme or url.netloc or not url.path:
                    continue
                target = (path.parent / unquote(url.path)).resolve()
                if target in DOCUMENTS:
                    pending.append(target)
        self.assertEqual(set(DOCUMENTS), visited)


if __name__ == "__main__":
    unittest.main()
