import json
import unittest
from html.parser import HTMLParser
from pathlib import Path

from ruff_ecosystem.html import render_html
from ruff_ecosystem.projects import ClonedRepository, Project, Repository, RuffCommand
from ruff_ecosystem.types import Comparison, Diff, Result


class Report(HTMLParser):
    def __init__(self, contents: str):
        super().__init__()
        self.tags = []
        self.payload = ""
        self.in_payload = False
        self.feed(contents)
        self.data = json.loads(self.payload)

    def handle_starttag(self, tag, attrs):
        self.tags.append(tag)
        if tag == "script":
            self.in_payload = dict(attrs).get("id") == "report-data"

    def handle_endtag(self, tag):
        if tag == "script":
            self.in_payload = False

    def handle_data(self, data):
        if self.in_payload:
            self.payload += data


def result(diff: Diff, *, error: str | None = None) -> Result:
    repo = Repository(owner="example", name="project", ref=None)
    cloned = ClonedRepository(
        owner=repo.owner, name=repo.name, ref=None, commit_hash="abc123", path=Path(".")
    )
    return Result(
        completed=[(Project(repo), Comparison(diff, cloned))],
        errored=[(Project(repo), RuntimeError(error))] if error else [],
    )


class HtmlReportTests(unittest.TestCase):
    def test_large_linter_report_keeps_every_change(self):
        lines = [f"+ src/file.py:{i}:1: F401 Unused import" for i in range(1, 1002)]
        lines.append("+ diagnostic that cannot be parsed")
        report = Report(
            render_html(result(Diff(lines)), RuffCommand.check, "check (stable)")
        )
        self.assertEqual(len(report.data["rows"]), 1002)
        self.assertEqual(report.data["rows"][-1]["text"], lines[-1])
        self.assertEqual(report.data["rows"][0]["rule"], "F401")
        self.assertEqual(
            report.data["rows"][0]["url"],
            "https://github.com/example/project/blob/abc123/src/file.py#L1",
        )
        self.assertEqual(report.data["added"], 1002)

    def test_report_text_cannot_break_out_of_json(self):
        text = '</script><svg onload="alert(1)"> & "quoted"'
        report = Report(
            render_html(
                result(Diff([f"+ {text}.py:1:1: F401 {text}"]), error=text),
                RuffCommand.check,
                text,
            )
        )
        self.assertNotIn("svg", report.tags)
        self.assertEqual(report.tags.count("script"), 2)
        self.assertEqual(report.data["title"], text)
        self.assertEqual(report.data["rows"][-1]["text"], text)
        self.assertEqual(report.data["rows"][-1]["kind"], "error")
        self.assertEqual(report.data["errors"], 1)
        self.assertIn("%3C", report.data["rows"][0]["url"])

    def test_formatter_keeps_context_and_both_sides(self):
        diff = Diff(
            [
                "--- a/src/example.py",
                "+++ b/src/example.py",
                "@@ -1,2 +1,2 @@",
                " unchanged",
                "-x=1",
                "+x = 1",
            ]
        )
        report = Report(
            render_html(result(diff), RuffCommand.format, "format (preview)")
        )
        rows = report.data["rows"]
        self.assertEqual([row["text"] for row in rows], diff.lines[2:])
        self.assertEqual(
            [row["kind"] for row in rows], ["context", "context", "removed", "added"]
        )
        self.assertEqual(rows[-1]["path"], "src/example.py")

    def test_unchanged_linter_output_is_not_a_change(self):
        report = Report(
            render_html(
                result(Diff.from_pair(["same"], ["same"])), RuffCommand.check, "check"
            )
        )
        self.assertEqual(report.data["rows"], [])
        self.assertEqual(report.data["changed"], 0)
        self.assertEqual(report.data["errors"], 0)


if __name__ == "__main__":
    unittest.main()
