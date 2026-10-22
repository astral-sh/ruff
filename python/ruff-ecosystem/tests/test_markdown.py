import unittest
from pathlib import Path

from ruff_ecosystem.check import markdown_check_result
from ruff_ecosystem.format import markdown_format_result
from ruff_ecosystem.markdown import markdown_sections
from ruff_ecosystem.projects import ClonedRepository, Project, Repository
from ruff_ecosystem.types import Comparison, Diff, Result


def comparison(name: str, lines: list[str]) -> tuple[Project, Comparison]:
    return (
        Project(Repository(owner="example", name=name, ref=None)),
        Comparison(
            Diff(lines),
            ClonedRepository(
                owner="example",
                name=name,
                ref=None,
                commit_hash="abc123",
                path=Path("."),
            ),
        ),
    )


def format_diff(count: int) -> list[str]:
    return [
        "--- a/example.py",
        "+++ b/example.py",
        f"@@ -1,{count} +1,{count} @@",
        *["-x=1"] * count,
        *["+x = 1"] * count,
    ]


class MarkdownReportTests(unittest.TestCase):
    def test_sections_are_kept_whole_and_later_sections_can_fit(self):
        large = "<details><summary>Large</summary>" + "x" * 1000 + "</details>"
        small = "<details><summary>Small</summary>\n\n```diff\n+x\n```\n</details>"
        rendered = markdown_sections("Summary", [large, small], 200)
        self.assertLessEqual(len(rendered), 200)
        self.assertIn(small, rendered)
        self.assertNotIn("Large", rendered)
        self.assertIn("omitted", rendered)

    def test_exact_limit_does_not_omit_details(self):
        sections = ["First", "Second"]
        complete = markdown_sections("Summary", sections)
        self.assertEqual(
            markdown_sections("Summary", sections, len(complete)), complete
        )

    def test_linter_summary_counts_omitted_projects(self):
        large = comparison(
            "large", [f"+ file.py:{i}:1: F401 Unused import" for i in range(1, 302)]
        )
        small = comparison("small", ["+ file.py:1:1: F401 Unused import"])
        error = Project(Repository(owner="example", name="error", ref=None))
        result = Result(
            completed=[large, small], errored=[(error, RuntimeError("failed"))]
        )
        rendered = markdown_check_result(result, 2000)
        self.assertLessEqual(len(rendered), 2000)
        self.assertIn("+302 -0 violations", rendered)
        self.assertIn("1 project error", rendered)
        self.assertIn("example/error", rendered)
        self.assertIn("example/small", rendered)
        self.assertIn("Changes by rule", rendered)
        self.assertIn("| F401 | 302 |", rendered)
        self.assertNotIn("example/large", rendered)
        self.assertEqual(rendered.count("<details>"), rendered.count("</details>"))
        complete = markdown_check_result(result)
        self.assertIn("file.py:301:1:", complete)
        self.assertNotIn("omitted", complete)

    def test_formatter_summary_keeps_later_projects_and_errors(self):
        large = comparison("large", format_diff(1000))
        small = comparison("small", format_diff(1))
        error = Project(Repository(owner="example", name="error", ref=None))
        result = Result(
            completed=[large, small], errored=[(error, RuntimeError("failed"))]
        )
        rendered = markdown_format_result(result, 1500)
        self.assertLessEqual(len(rendered), 1500)
        self.assertIn("+1001 -1001 lines", rendered)
        self.assertIn("1 project error", rendered)
        self.assertIn("example/error", rendered)
        self.assertIn("example/small", rendered)
        self.assertNotIn("example/large", rendered)
        self.assertEqual(rendered.count("<details>"), rendered.count("</details>"))
        self.assertEqual(rendered.count("```"), 4)
        self.assertGreater(len(markdown_format_result(result)), 1500)

    def test_removed_lines_are_reported_as_changes(self):
        result = Result(
            completed=[
                comparison(
                    "project",
                    ["--- a/example.py", "+++ b/example.py", "@@ -1 +0,0 @@", "-x=1"],
                )
            ],
            errored=[],
        )
        self.assertIn("+0 -1 lines", markdown_format_result(result, 512))

    def test_oversized_error_keeps_the_error_count(self):
        project = Project(Repository(owner="example", name="error", ref=None))
        result = Result(completed=[], errored=[(project, RuntimeError("x" * 10000))])
        for render in [markdown_check_result, markdown_format_result]:
            with self.subTest(renderer=render.__name__):
                rendered = render(result, 512)
                self.assertLessEqual(len(rendered), 512)
                self.assertIn("1 project error", rendered)
                self.assertIn("omitted", rendered)


if __name__ == "__main__":
    unittest.main()
