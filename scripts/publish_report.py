# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Publish report artifacts without executing code from the producing workflow."""

from __future__ import annotations

import argparse
import gzip
import io
import json
import logging
import os
import re
import subprocess
import sys
import zipfile
from pathlib import Path
from typing import Any
from urllib.error import HTTPError
from urllib.request import Request, urlopen

logger = logging.getLogger(__name__)

REPOSITORY = "astral-sh/ruff"
REPOSITORY_ID = 523043277
API_ROOT = f"https://api.github.com/repos/{REPOSITORY}"
BOT_ID = 305554984  # astral-automations-bot

MAX_ARCHIVE_BYTES = 16 * 1024 * 1024
MAX_REPORT_BYTES = 4 * 1024 * 1024
MAX_REPORT_CHARACTERS = 60_000
MAX_HTML_ARCHIVE_BYTES = 128 * 1024 * 1024
MAX_HTML_REPORT_BYTES = 512 * 1024 * 1024


def read_html_files(value: str) -> dict[str, tuple[str, str]]:
    files = json.loads(value)
    if not isinstance(files, dict):
        raise TypeError("HTML files must be a JSON object keyed by filename.")
    result = {}
    for filename, metadata in files.items():
        if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}", filename):
            raise ValueError(
                "HTML filenames must be plain filenames without directories."
            )
        if not isinstance(metadata, dict) or metadata.keys() - {
            "content-type",
            "label",
        }:
            raise ValueError("HTML files accept a content-type and an optional label.")
        content_type = metadata.get("content-type", "")
        label = metadata.get("label", "")
        if not isinstance(content_type, str) or not re.fullmatch(
            r"[A-Za-z0-9.+-]+/[A-Za-z0-9.+-]+(?:; charset=utf-8)?", content_type
        ):
            raise ValueError("Invalid HTML report content type.")
        if not isinstance(label, str) or not re.fullmatch(r"[^\r\n]{0,100}", label):
            raise ValueError(
                "HTML report labels must be at most 100 characters on one line."
            )
        result[filename] = (content_type, label)
    return result


def report_marker(report: str) -> str:
    return f"<!-- generated-comment {report} -->"


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


def matches_comment(comment: dict, pull_request: int, marker: str) -> bool:
    author = comment.get("user") or {}
    return (
        author.get("id") == BOT_ID
        and author.get("type") == "Bot"
        and comment.get("issue_url") == f"{API_ROOT}/issues/{pull_request}"
        and (comment.get("body") or "").startswith(f"{marker}\n")
    )


def read_comment_id(path: str) -> int | None:
    try:
        value = Path(path).read_text(encoding="utf-8")
    except FileNotFoundError:
        return None
    # Stored values are untrusted, including values from other jobs on the same PR.
    if not re.fullmatch(r"[1-9][0-9]{0,19}", value):
        raise ValueError("The stored comment ID must be a positive decimal integer.")
    return int(value)


def find_report(
    pull_request: int, report: str, comment_id: int | None = None
) -> dict | None:
    if comment_id is None:
        return None
    comment = api(f"issues/comments/{comment_id}", missing_ok=True)
    if comment is not None and not matches_comment(
        comment, pull_request, report_marker(report)
    ):
        raise ValueError(
            "The stored ID does not identify this bot's report on this PR."
        )
    return comment


def find_artifact(run_id: int, name: str) -> dict | None:
    matches = []
    page = 1
    while True:
        artifacts = api(f"actions/runs/{run_id}/artifacts?per_page=100&page={page}")[
            "artifacts"
        ]
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
            f"repos/{REPOSITORY}/actions/artifacts/{artifact['id']}/zip",
        ],
        check=True,
        stdout=subprocess.PIPE,
        timeout=120,
    )
    if len(result.stdout) > max_bytes:
        raise ValueError("The report archive exceeds the download limit.")
    return result.stdout


def read_report(
    archive_bytes: bytes, filename: str, *, max_bytes: int = MAX_REPORT_BYTES
) -> str:
    # Read one fixed member in memory. Archive paths are never extracted, and
    # artifacts cannot replace the publisher or supply executable configuration.
    with zipfile.ZipFile(io.BytesIO(archive_bytes)) as archive:
        members = [info for info in archive.infolist() if info.filename == filename]
        if len(members) != 1 or members[0].is_dir():
            raise ValueError(f"Expected exactly one {filename} report in the archive.")
        if members[0].file_size > max_bytes:
            raise ValueError("The uncompressed report exceeds the size limit.")
        with archive.open(members[0]) as member:
            data = member.read(max_bytes + 1)
        if len(data) > max_bytes:
            raise ValueError("The uncompressed report exceeds the size limit.")
        return data.decode("utf-8")


def upload_html_report(
    files: dict[str, tuple[str, str]],
    artifact: dict,
    *,
    prefix: str,
    bucket: str,
    base_url: str,
) -> str:
    if not files or not base_url:
        raise ValueError(
            "HTML report files and their public base URL must be configured."
        )
    archive = download_artifact(artifact, max_bytes=MAX_HTML_ARCHIVE_BYTES)
    reports = {}
    total_bytes = 0
    for filename in files:
        contents = read_report(
            archive, filename, max_bytes=MAX_HTML_REPORT_BYTES
        ).encode()
        total_bytes += len(contents)
        if total_bytes > MAX_HTML_REPORT_BYTES:
            raise ValueError("The uncompressed HTML report exceeds the size limit.")
        reports[filename] = contents
    for filename, contents in reports.items():
        subprocess.run(
            [
                "aws",
                "s3",
                "cp",
                "-",
                f"s3://{bucket}/{prefix}/{filename}",
                "--content-type",
                files[filename][0],
                "--content-encoding",
                "gzip",
                "--cache-control",
                "public, max-age=3600",
                "--only-show-errors",
            ],
            input=gzip.compress(contents, compresslevel=6, mtime=0),
            check=True,
            timeout=120,
        )
    return f"{base_url.rstrip('/')}/{prefix}"


def render_report(
    report: str,
    contents: str,
    run: dict,
    artifact: dict,
    *,
    title: str = "",
    full_report: dict | None = None,
    full_report_url: str = "",
    html_files: dict[str, tuple[str, str]] | None = None,
) -> str:
    if len(contents) > MAX_REPORT_CHARACTERS:
        artifact_url = f"https://github.com/{REPOSITORY}/actions/runs/{run['id']}/artifacts/{artifact['id']}"
        contents = (
            contents[:MAX_REPORT_CHARACTERS]
            + f"\n\nReport truncated. [Download the untruncated content (ZIP)]({artifact_url})."
        )
    if title:
        contents = f"## {title}\n\n{contents}"
    if full_report_url:
        links = []
        for filename, (_, label) in (html_files or {}).items():
            if label:
                label = (
                    label.replace("\\", "\\\\").replace("[", "\\[").replace("]", "\\]")
                )
                links.append(f"[{label}]({full_report_url}/{filename})")
        if links:
            contents = f"{contents.rstrip()}\n\n" + " · ".join(links)
    if full_report is not None:
        report_url = f"https://github.com/{REPOSITORY}/actions/runs/{run['id']}/artifacts/{full_report['id']}"
        contents = (
            f"{contents.rstrip()}\n\n[Download the full report (ZIP)]({report_url})"
        )
    commit_url = f"https://github.com/{REPOSITORY}/commit/{run['head_sha']}"
    run_url = f"https://github.com/{REPOSITORY}/actions/runs/{run['id']}/attempts/{run['run_attempt']}"
    return (
        f"{report_marker(report)}\n"
        f"<!-- report-run:{run['id']}:{run['run_attempt']} -->\n\n"
        f"{contents.rstrip()}\n\n---\n\n"
        f"Results for [{run['head_sha'][:7]}]({commit_url}) · [Workflow run]({run_url})\n"
    )


def report_run(comment: dict) -> tuple[int, int]:
    lines = comment["body"].splitlines()
    if len(lines) > 1 and (
        match := re.fullmatch(
            r"<!-- report-run:([1-9][0-9]*):([1-9][0-9]*) -->", lines[1]
        )
    ):
        return int(match[1]), int(match[2])
    raise ValueError("The stored report comment has no valid workflow run metadata.")


def current_run(
    run_id: int, run_attempt: int, workflow_ref: str, pull_request: int
) -> dict | None:
    run = api(f"actions/runs/{run_id}")
    if run["run_attempt"] != run_attempt:
        print("The workflow run has been rerun.", file=sys.stderr)
        return None
    if (
        run["repository"]["id"] != REPOSITORY_ID
        or (run["head_repository"] or {}).get("id") != REPOSITORY_ID
        or run["event"] != "pull_request"
        or workflow_ref != f"{REPOSITORY}/{run['path']}@refs/pull/{pull_request}/merge"
    ):
        raise ValueError("The workflow run is not an eligible same-repository PR run.")
    pull = api(f"pulls/{pull_request}")
    if pull["number"] != pull_request:
        raise ValueError("The run does not belong to the requested pull request.")
    if (
        pull["state"] != "open"
        or pull["base"]["repo"]["id"] != REPOSITORY_ID
        or (pull["head"]["repo"] or {}).get("id") != REPOSITORY_ID
        or pull["head"]["sha"] != run["head_sha"]
        or pull["head"]["ref"] != run["head_branch"]
    ):
        print("The run is not for the current head of this PR.", file=sys.stderr)
        return None
    return run


def publish(
    run_id: int,
    run_attempt: int,
    report: str,
    pull_request: int,
    *,
    workflow_ref: str,
    artifact_name: str,
    filename: str,
    title: str = "",
    full_report_artifact: str = "",
    html_bucket: str = "",
    html_base_url: str = "",
    html_files: dict[str, tuple[str, str]] | None = None,
    stored_comment_id: int | None = None,
    dry_run: bool = False,
) -> int | None:
    if not (run := current_run(run_id, run_attempt, workflow_ref, pull_request)):
        return
    artifact = find_artifact(run_id, artifact_name)
    if artifact is None:
        print(f"No {artifact_name} artifact is available.", file=sys.stderr)
        return
    contents = read_report(download_artifact(artifact), filename)
    if not contents.strip():
        return
    full_report = (
        find_artifact(run_id, full_report_artifact) if full_report_artifact else None
    )
    full_report_url = ""
    if full_report is not None and html_bucket and not dry_run:
        try:
            full_report_url = upload_html_report(
                html_files or {},
                full_report,
                prefix=f"ruff/pull-requests/{pull_request}/{report}/{run_id}/{run_attempt}",
                bucket=html_bucket,
                base_url=html_base_url,
            )
        except Exception:
            # An upload failure must not suppress the summary or artifact link.
            logger.exception("Failed to upload the HTML report.")
    body = render_report(
        report,
        contents,
        run,
        artifact,
        title=title,
        full_report=full_report,
        full_report_url=full_report_url,
        html_files=html_files,
    )
    if dry_run:
        print(body, end="")
        return

    # Recheck after downloading: the PR may have advanced or the run may have been retried.
    if not current_run(run_id, run_attempt, workflow_ref, pull_request):
        return
    existing = find_report(pull_request, report, stored_comment_id)
    if existing and report_run(existing) > (run_id, run_attempt):
        print("A newer run already published this report.", file=sys.stderr)
        return existing["id"]
    if existing:
        comment_id = existing["id"]
        if existing["body"] != body:
            api(f"issues/comments/{comment_id}", method="PATCH", body=body)
    else:
        comment_id = api(f"issues/{pull_request}/comments", method="POST", body=body)[
            "id"
        ]
    print(f"Published {report} report: {comment_id}")
    return comment_id


def publish_weekly_report(
    run_id: int,
    run_attempt: int,
    workflow_ref: str,
    *,
    html_files: dict[str, tuple[str, str]],
    bucket: str,
    base_url: str,
) -> str | None:
    run = api(f"actions/runs/{run_id}")
    if run["run_attempt"] != run_attempt:
        print("The workflow run has been rerun.", file=sys.stderr)
        return None
    if (
        run["repository"]["id"] != REPOSITORY_ID
        or (run["head_repository"] or {}).get("id") != REPOSITORY_ID
        or run["event"] not in {"schedule", "workflow_dispatch"}
        or run["head_branch"] != "main"
        or run["path"] != ".github/workflows/ty-ecosystem-report.yaml"
        or workflow_ref != f"{REPOSITORY}/{run['path']}@refs/heads/main"
    ):
        raise ValueError(
            "Only weekly ty ecosystem runs from main can publish this report."
        )
    latest_runs = api(
        "actions/workflows/ty-ecosystem-report.yaml/runs?branch=main&per_page=1"
    )["workflow_runs"]
    if latest_runs and latest_runs[0]["id"] > run_id:
        print("A newer weekly report run exists.", file=sys.stderr)
        return None
    artifact = find_artifact(run_id, "full-report")
    if artifact is None:
        raise ValueError("No full-report artifact is available.")
    if not bucket:
        raise ValueError("The report bucket must be configured.")
    report_url = upload_html_report(
        html_files,
        artifact,
        prefix="ruff/ty-ecosystem",
        bucket=bucket,
        base_url=base_url,
    )
    return f"{report_url}/index.html"


def main() -> None:
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--run-id", required=True, type=int)
    common.add_argument("--run-attempt", required=True, type=int)
    common.add_argument("--workflow-ref", required=True)
    common.add_argument("--html-bucket", default="")
    common.add_argument("--html-base-url", default="")
    common.add_argument("--html-files", type=read_html_files, default="{}")
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    comment = commands.add_parser("comment", parents=[common])
    comment.add_argument("--report", required=True)
    comment.add_argument("--pull-request", required=True, type=int)
    comment.add_argument("--artifact", required=True)
    comment.add_argument("--filename", required=True)
    comment.add_argument("--title", default="")
    comment.add_argument("--full-report-artifact", default="")
    comment.add_argument("--comment-id-file", dest="comment_id", type=read_comment_id)
    comment.add_argument("--dry-run", action="store_true")
    commands.add_parser("weekly", parents=[common])
    args = parser.parse_args()
    if args.run_id <= 0 or args.run_attempt <= 0:
        parser.error("The run ID and attempt must be positive.")
    if args.command == "weekly":
        report_url = publish_weekly_report(
            args.run_id,
            args.run_attempt,
            args.workflow_ref,
            html_files=args.html_files,
            bucket=args.html_bucket,
            base_url=args.html_base_url,
        )
        if report_url is not None and (
            summary := os.environ.get("GITHUB_STEP_SUMMARY")
        ):
            with Path(summary).open("a") as file:
                file.write(f"[Full ty ecosystem report]({report_url})\n")
        return
    if args.pull_request <= 0:
        parser.error("The pull request number must be positive.")
    comment_id = publish(
        args.run_id,
        args.run_attempt,
        args.report,
        args.pull_request,
        workflow_ref=args.workflow_ref,
        artifact_name=args.artifact,
        filename=args.filename,
        title=args.title,
        full_report_artifact=args.full_report_artifact,
        html_bucket=args.html_bucket,
        html_base_url=args.html_base_url,
        html_files=args.html_files,
        stored_comment_id=args.comment_id,
        dry_run=args.dry_run,
    )
    if comment_id is not None and (output := os.environ.get("GITHUB_OUTPUT")):
        with Path(output).open("a") as file:
            file.write(f"comment-id={comment_id:d}\n")


if __name__ == "__main__":
    main()
