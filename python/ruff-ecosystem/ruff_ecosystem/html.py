"""Render ecosystem changes as a searchable HTML report."""

from __future__ import annotations

import json
from importlib.resources import files
from typing import TYPE_CHECKING, TypedDict
from urllib.parse import quote

from unidiff import PatchSet

from ruff_ecosystem.check import CHECK_DIFF_LINE_RE, DiagnosticLine
from ruff_ecosystem.projects import RuffCommand

if TYPE_CHECKING:
    from ruff_ecosystem.types import Result


class ReportRow(TypedDict):
    project: str
    path: str
    url: str
    text: str
    kind: str
    rule: str


def render_html(result: Result, command: RuffCommand, title: str) -> str:
    rows: list[ReportRow] = []
    for project, comparison in result.completed:
        repo = comparison.repo
        if command == RuffCommand.check:
            for line in comparison.diff.without_unchanged_lines():
                diagnostic = DiagnosticLine.try_from_string(line)
                match = CHECK_DIFF_LINE_RE.match(line)
                path = match["path"] if match else ""
                rows.append(
                    ReportRow(
                        project=project.repo.fullname,
                        path=path,
                        url=repo.url_for(quote(path), int(match["lnum"]))
                        if match
                        else "",
                        text=line,
                        kind="added" if line.startswith("+") else "removed",
                        rule=diagnostic.rule_code if diagnostic else "",
                    )
                )
        else:
            for file in PatchSet("\n".join(comparison.diff.lines)):
                for hunk in file:
                    # The baseline may have been formatted, so source lines are approximate.
                    url = repo.url_for(quote(file.path), hunk.source_start)
                    for line in str(hunk).splitlines():
                        rows.append(
                            ReportRow(
                                project=project.repo.fullname,
                                path=file.path,
                                url=url,
                                text=line,
                                kind="added"
                                if line.startswith("+")
                                else "removed"
                                if line.startswith("-")
                                else "context",
                                rule="",
                            )
                        )
    for project, error in result.errored:
        rows.append(
            ReportRow(
                project=project.repo.fullname,
                path="",
                url="",
                text=str(error),
                kind="error",
                rule="",
            )
        )
    data = json.dumps(
        {
            "title": title,
            "command": command.value,
            "projects": len(result.completed) + len(result.errored),
            "changed": sum(bool(comparison.diff) for _, comparison in result.completed),
            "added": sum(
                comparison.diff.lines_added for _, comparison in result.completed
            ),
            "removed": sum(
                comparison.diff.lines_removed for _, comparison in result.completed
            ),
            "errors": len(result.errored),
            "rows": rows,
        },
        ensure_ascii=False,
        separators=(",", ":"),
    ).replace("<", "\\u003c")
    template = (
        files("ruff_ecosystem").joinpath("report.html").read_text(encoding="utf-8")
    )
    return template.replace("<!-- report-data -->", data)
