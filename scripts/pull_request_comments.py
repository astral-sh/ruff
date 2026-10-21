# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Reserve an early automation index and look up its report comments."""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from typing import Any, NamedTuple
from urllib.error import HTTPError
from urllib.parse import urlencode
from urllib.request import Request, urlopen

REPOSITORY = "astral-sh/ruff"
REPOSITORY_ID = 523043277
API_ROOT = f"https://api.github.com/repos/{REPOSITORY}"
COMMENT_LIMIT = 3
INDEX_MARKER = "<!-- ruff-automation-reports -->"
BOT_IDS = {41898282, 305554984}  # github-actions and astral-automations-bot


class Report(NamedTuple):
    title: str
    marker: str
    workflow: str


REPORTS = {
    "ruff-ecosystem": Report(
        "Ruff ecosystem", "<!-- generated-comment ecosystem -->", "ci.yaml"
    ),
    "memory": Report(
        "Memory", "<!-- generated-comment memory_report_diff -->", "memory_report.yaml"
    ),
    "typing-conformance": Report(
        "Typing conformance",
        "<!-- generated-comment typing_conformance_diagnostics_diff -->",
        "typing_conformance.yaml",
    ),
    "ty-ecosystem": Report(
        "ty ecosystem",
        "<!-- generated-comment ty ecosystem-analyzer -->",
        "ty-ecosystem-analyzer.yaml",
    ),
}


def api(
    endpoint: str,
    *,
    method: str = "GET",
    body: str | None = None,
    missing_ok: bool = False,
) -> Any:
    request = Request(
        f"{API_ROOT}/{endpoint}",
        data=json.dumps({"body": body}).encode() if body is not None else None,
        headers={
            "Authorization": f"Bearer {os.environ['GH_TOKEN']}",
            "Accept": "application/vnd.github+json",
            "Content-Type": "application/json",
            "X-GitHub-Api-Version": "2022-11-28",
        },
        method=method,
    )
    try:
        with urlopen(request, timeout=30) as response:
            return json.load(response)
    except HTTPError as error:
        if missing_ok and error.code == 404:
            return None
        raise


def pages(endpoint: str):
    page = 1
    separator = "&" if "?" in endpoint else "?"
    while True:
        items = api(f"{endpoint}{separator}per_page=100&page={page}")
        yield from items
        if len(items) < 100:
            return
        page += 1


def pull_request_for_run(run_id: int, workflow: str) -> dict | None:
    run = api(f"actions/runs/{run_id}")
    if (
        run["repository"]["id"] != REPOSITORY_ID
        or run["event"] != "pull_request"
        or run["path"] != f".github/workflows/{workflow}"
    ):
        raise ValueError("The workflow run is not an eligible pull request run.")

    # Fork runs can have no associated PRs, including in the commit API. Find
    # their source branch, then verify the repository and exact head SHA below.
    candidates = run["pull_requests"]
    if not candidates and run["head_repository"]:
        owner = run["head_repository"]["full_name"].split("/")[0]
        query = urlencode({"state": "open", "head": f"{owner}:{run['head_branch']}"})
        candidates = list(pages(f"pulls?{query}"))
    matches = []
    for candidate in candidates:
        pull = api(f"pulls/{candidate['number']}")
        if (
            pull["state"] == "open"
            and pull["base"]["repo"]["id"] == REPOSITORY_ID
            and (pull["head"]["repo"] or {}).get("id")
            == (run["head_repository"] or {}).get("id")
            and pull["head"]["ref"] == run["head_branch"]
            and pull["head"]["sha"] == run["head_sha"]
        ):
            matches.append(pull)
    if len(matches) == 1:
        return matches[0]
    print("No unambiguous, current open pull request for this run.", file=sys.stderr)
    return None


def matches_comment(comment: dict, pull_request: int, marker: str) -> bool:
    author = comment.get("user") or {}
    return (
        author.get("id") in BOT_IDS
        and author.get("type") == "Bot"
        and comment.get("issue_url") == f"{API_ROOT}/issues/{pull_request}"
        and (comment.get("body") or "").startswith(f"{marker}\n")
    )


def first_comments(pull_request: int) -> list[dict]:
    return api(f"issues/{pull_request}/comments?per_page={COMMENT_LIMIT}")


def find_index(pull_request: int, comments: list[dict] | None = None) -> dict | None:
    if comments is None:
        comments = first_comments(pull_request)
    return next(
        (c for c in comments if matches_comment(c, pull_request, INDEX_MARKER)), None
    )


def index_entries(comment: dict) -> dict[str, int]:
    return {
        key: int(comment_id)
        for key, comment_id in re.findall(
            r"^<!-- report:([a-z-]+):([1-9][0-9]*) -->$",
            comment["body"],
            flags=re.MULTILINE,
        )
        if key in REPORTS
    }


def index_body(pull_request: int, entries: dict[str, int]) -> str:
    lines = [INDEX_MARKER, "## Automation reports", ""]
    for key, report in REPORTS.items():
        if comment_id := entries.get(key):
            url = f"https://github.com/{REPOSITORY}/pull/{pull_request}#issuecomment-{comment_id}"
            lines.extend(
                [f"<!-- report:{key}:{comment_id} -->", f"- [{report.title}]({url})"]
            )
    if not entries:
        lines.append(
            "Links to automation reports will appear here as they become available."
        )
    return "\n".join(lines) + "\n"


def reserve_index(pull_request: int) -> dict | None:
    pull = api(f"pulls/{pull_request}")
    if pull["state"] != "open" or pull["base"]["repo"]["id"] != REPOSITORY_ID:
        return None
    comments = first_comments(pull_request)
    if index := find_index(pull_request, comments):
        return index
    if len(comments) >= COMMENT_LIMIT:
        print("The first three comment positions are occupied.", file=sys.stderr)
        return None
    return api(
        f"issues/{pull_request}/comments",
        method="POST",
        body=index_body(pull_request, {}),
    )


def find_report(pull_request: int, report: str) -> dict | None:
    if (index := find_index(pull_request)) and (
        comment_id := index_entries(index).get(report)
    ):
        comment = api(f"issues/comments/{comment_id}", missing_ok=True)
        if comment and matches_comment(comment, pull_request, REPORTS[report].marker):
            return comment
    # Older PRs may have no early index. Discovery also recovers deleted reports
    # and a successful comment creation followed by a failed index update.
    return next(
        (
            comment
            for comment in pages(f"issues/{pull_request}/comments")
            if matches_comment(comment, pull_request, REPORTS[report].marker)
        ),
        None,
    )


def register_report(pull_request: int, report: str, comment_id: int) -> None:
    comment = api(f"issues/comments/{comment_id}")
    if not matches_comment(comment, pull_request, REPORTS[report].marker):
        raise ValueError("The report comment has the wrong author, PR, or marker.")
    index = find_index(pull_request)
    if index is None:
        # A human comment can race the reservation into a later position.
        index = next(
            (
                comment
                for comment in pages(f"issues/{pull_request}/comments")
                if matches_comment(comment, pull_request, INDEX_MARKER)
            ),
            None,
        )
    if index is None:
        return
    entries = index_entries(index)
    entries[report] = comment_id
    body = index_body(pull_request, entries)
    if body != index["body"]:
        api(f"issues/comments/{index['id']}", method="PATCH", body=body)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    resolve = subparsers.add_parser("resolve")
    resolve.add_argument("--run-id", required=True, type=int)
    resolve.add_argument(
        "--workflow", required=True, choices=[r.workflow for r in REPORTS.values()]
    )
    for command in ("reserve", "find", "lookup", "register"):
        subparser = subparsers.add_parser(command)
        subparser.add_argument("--pull-request", required=True, type=int)
        if command in {"lookup", "register"}:
            subparser.add_argument("--report", required=True, choices=REPORTS)
        if command == "register":
            subparser.add_argument("--comment-id", required=True, type=int)
    args = parser.parse_args()
    if os.environ.get("GITHUB_REPOSITORY", REPOSITORY) != REPOSITORY:
        parser.error(f"This script only supports {REPOSITORY}.")
    for name in ("pull_request", "run_id", "comment_id"):
        if (value := getattr(args, name, None)) is not None and value <= 0:
            parser.error(f"{name} must be positive")
    if args.command == "resolve":
        if pull := pull_request_for_run(args.run_id, args.workflow):
            print(pull["number"])
    elif args.command == "register":
        register_report(args.pull_request, args.report, args.comment_id)
    else:
        if args.command == "reserve":
            comment = reserve_index(args.pull_request)
        elif args.command == "lookup":
            comment = find_report(args.pull_request, args.report)
        else:
            comment = find_index(args.pull_request)
        if comment:
            print(comment["id"])


if __name__ == "__main__":
    main()
