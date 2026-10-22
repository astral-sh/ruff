# /// script
# requires-python = ">=3.10"
# dependencies = []
#
# [tool.ty.environment]
# root = ["."]
# ///
"""Publish report artifacts without executing code from the producing workflow."""

from __future__ import annotations

import argparse
import io
import os
import re
import subprocess
import sys
import zipfile
from pathlib import Path

import pull_request_comments as comments

ARTIFACTS = {
    "ruff-ecosystem": ("ecosystem-result", "ecosystem-result"),
    "memory": ("memory_report_diff", "memory_report_diff.diff"),
    "typing-conformance": (
        "typing_conformance_diagnostics_diff",
        "typing_conformance_diagnostics.diff",
    ),
    "ty-ecosystem": ("comment.md", "comment.md"),
}
MAX_ARCHIVE_BYTES = 16 * 1024 * 1024
MAX_REPORT_BYTES = 4 * 1024 * 1024
MAX_REPORT_CHARACTERS = 60_000


def find_artifact(run_id: int, name: str) -> dict | None:
    matches = []
    page = 1
    while True:
        artifacts = comments.api(
            f"actions/runs/{run_id}/artifacts?per_page=100&page={page}"
        )["artifacts"]
        matches.extend(
            artifact
            for artifact in artifacts
            if artifact["name"] == name and not artifact["expired"]
        )
        if len(artifacts) < 100:
            break
        page += 1
    # A rerun can leave artifacts from multiple attempts under the same name.
    return max(matches, key=lambda artifact: artifact["id"]) if matches else None


def download_artifact(artifact: dict, *, max_bytes: int = MAX_ARCHIVE_BYTES) -> bytes:
    if artifact["size_in_bytes"] > max_bytes:
        raise ValueError("The report archive exceeds the download limit.")
    result = subprocess.run(
        [
            "gh",
            "api",
            f"repos/{comments.REPOSITORY}/actions/artifacts/{artifact['id']}/zip",
        ],
        check=True,
        stdout=subprocess.PIPE,
        timeout=120,
    )
    if len(result.stdout) > max_bytes:
        raise ValueError("The report archive exceeds the download limit.")
    return result.stdout


def read_report(archive_bytes: bytes, filename: str) -> str:
    # Read one fixed member in memory. Archive paths are never extracted, and
    # artifacts cannot replace the publisher or supply executable configuration.
    with zipfile.ZipFile(io.BytesIO(archive_bytes)) as archive:
        members = [info for info in archive.infolist() if info.filename == filename]
        if len(members) != 1 or members[0].is_dir():
            raise ValueError(f"Expected exactly one {filename} report in the archive.")
        if members[0].file_size > MAX_REPORT_BYTES:
            raise ValueError("The uncompressed report exceeds the size limit.")
        with archive.open(members[0]) as member:
            data = member.read(MAX_REPORT_BYTES + 1)
        if len(data) > MAX_REPORT_BYTES:
            raise ValueError("The uncompressed report exceeds the size limit.")
        return data.decode("utf-8")


def render_report(
    report: str,
    contents: str,
    run: dict,
    artifact: dict,
    *,
    full_report: dict | None = None,
) -> str:
    if len(contents) > MAX_REPORT_CHARACTERS:
        artifact_url = f"https://github.com/{comments.REPOSITORY}/actions/runs/{run['id']}/artifacts/{artifact['id']}"
        contents = (
            contents[:MAX_REPORT_CHARACTERS]
            + f"\n\nReport truncated. [Download the full report]({artifact_url})."
        )
    if report == "ruff-ecosystem":
        contents = f"## `ruff-ecosystem` results\n\n{contents}"
    if full_report is not None:
        report_url = f"https://github.com/{comments.REPOSITORY}/actions/runs/{run['id']}/artifacts/{full_report['id']}"
        contents = (
            f"{contents.rstrip()}\n\n[Download the HTML reports (ZIP)]({report_url})"
        )
    run_url = f"https://github.com/{comments.REPOSITORY}/actions/runs/{run['id']}/attempts/{run['run_attempt']}"
    return (
        f"{comments.REPORTS[report].marker}\n"
        f"<!-- report-run:{run['id']}:{run['run_attempt']} -->\n\n"
        f"{contents.rstrip()}\n\n"
        f"[Workflow run]({run_url}) · `{run['head_sha'][:7]}`\n"
    )


def report_run(comment: dict) -> tuple[int, int]:
    lines = comment["body"].splitlines()
    if len(lines) > 1 and (
        match := re.fullmatch(r"<!-- report-run:([0-9]+):([0-9]+) -->", lines[1])
    ):
        return int(match[1]), int(match[2])
    return 0, 0


def current_run(
    run_id: int, run_attempt: int, report: str, pull_request: int
) -> tuple[dict, dict] | None:
    run = comments.api(f"actions/runs/{run_id}")
    if run["run_attempt"] != run_attempt:
        print("The workflow run has been rerun.", file=sys.stderr)
        return None
    if (
        run["repository"]["id"] != comments.REPOSITORY_ID
        or (run["head_repository"] or {}).get("id") != comments.REPOSITORY_ID
        or run["event"] != "pull_request"
        or run["path"] != f".github/workflows/{comments.REPORTS[report].workflow}"
    ):
        raise ValueError("The workflow run is not an eligible same-repository PR run.")
    pull = comments.api(f"pulls/{pull_request}")
    if (
        pull["state"] != "open"
        or pull["base"]["repo"]["id"] != comments.REPOSITORY_ID
        or (pull["head"]["repo"] or {}).get("id") != comments.REPOSITORY_ID
        or pull["head"]["sha"] != run["head_sha"]
        or pull["head"]["ref"] != run["head_branch"]
    ):
        print("The run is not for the current head of this PR.", file=sys.stderr)
        return None
    return run, pull


def publish(
    run_id: int,
    run_attempt: int,
    report: str,
    pull_request: int,
    *,
    stored_comment_id: int | None = None,
    dry_run: bool = False,
) -> int | None:
    if not (source := current_run(run_id, run_attempt, report, pull_request)):
        return
    run, pull = source
    if pull["number"] != pull_request:
        raise ValueError("The run does not belong to the requested pull request.")
    artifact_name, filename = ARTIFACTS[report]
    artifact = find_artifact(run_id, artifact_name)
    if artifact is None:
        print(f"No {artifact_name} artifact is available.", file=sys.stderr)
        return
    contents = read_report(download_artifact(artifact), filename)
    if not contents.strip():
        return
    full_report = (
        find_artifact(run_id, "full-report") if report == "ty-ecosystem" else None
    )
    body = render_report(report, contents, run, artifact, full_report=full_report)
    if dry_run:
        print(body, end="")
        return

    # Recheck after downloading: the PR may have advanced or the run may have been retried.
    current = current_run(run_id, run_attempt, report, pull_request)
    if not current or current[1]["number"] != pull_request:
        return
    existing = comments.find_report(pull_request, report, stored_comment_id)
    if existing and report_run(existing) > (run_id, run_attempt):
        print("A newer run already published this report.", file=sys.stderr)
        return existing["id"]
    if existing:
        comment_id = existing["id"]
        if existing["body"] != body:
            comments.api(f"issues/comments/{comment_id}", method="PATCH", body=body)
    else:
        comment_id = comments.api(
            f"issues/{pull_request}/comments", method="POST", body=body
        )["id"]
    print(f"Published {report} report: {comment_id}")
    return comment_id


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-id", required=True, type=int)
    parser.add_argument("--run-attempt", required=True, type=int)
    parser.add_argument("--report", required=True, choices=ARTIFACTS)
    parser.add_argument("--pull-request", required=True, type=int)
    parser.add_argument("--comment-id", type=comments.parse_comment_id)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    if args.run_id <= 0 or args.run_attempt <= 0:
        parser.error("The run ID and attempt must be positive.")
    if args.pull_request <= 0:
        parser.error("The pull request number must be positive.")
    comment_id = publish(
        args.run_id,
        args.run_attempt,
        args.report,
        args.pull_request,
        stored_comment_id=args.comment_id,
        dry_run=args.dry_run,
    )
    if comment_id is not None and (output := os.environ.get("GITHUB_OUTPUT")):
        with Path(output).open("a") as file:
            file.write(f"comment-id={comment_id:d}\n")


if __name__ == "__main__":
    main()
