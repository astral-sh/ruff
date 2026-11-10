# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Publish report artifacts without executing code from the producing workflow."""

from __future__ import annotations

import argparse
import io
import json
import logging
import os
import re
import subprocess
import sys
import zipfile
from collections.abc import Iterator
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from urllib.error import HTTPError
from urllib.parse import urlencode
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
MAX_HTML_REPORT_BYTES = 128 * 1024 * 1024


@dataclass(frozen=True, kw_only=True)
class ForkReport:
    name: str
    artifacts: dict[str, str]


FORK_REPORTS = {
    ".github/workflows/ty-ecosystem-analyzer.yaml": ForkReport(
        name="ty-ecosystem",
        artifacts={
            "comment.md": "Download the ty ecosystem summary (ZIP)",
            "full-report": "Download the full ty ecosystem report (ZIP)",
        },
    ),
    ".github/workflows/typing_conformance.yaml": ForkReport(
        name="typing-conformance",
        artifacts={
            "typing_conformance_diagnostics_diff": "Download the typing conformance report (ZIP)"
        },
    ),
    ".github/workflows/memory_report.yaml": ForkReport(
        name="memory",
        artifacts={"memory_report_diff": "Download the memory usage report (ZIP)"},
    ),
}


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


def api_items(endpoint: str, **params: str) -> Iterator[dict]:
    page = 1
    while True:
        query = urlencode({**params, "per_page": 100, "page": page})
        items = api(f"{endpoint}?{query}")
        yield from items
        if len(items) < 100:
            return
        page += 1


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


def find_artifact(run_id: int, name: str, *, since: str | None = None) -> dict | None:
    matches = []
    page = 1
    while True:
        query = urlencode({"name": name, "per_page": 100, "page": page})
        artifacts = api(f"actions/runs/{run_id}/artifacts?{query}")["artifacts"]
        matches.extend(
            artifact
            for artifact in artifacts
            if artifact["name"] == name
            and not artifact["expired"]
            and (since is None or artifact["created_at"] >= since)
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
                "--cache-control",
                "public, max-age=3600",
                "--only-show-errors",
            ],
            input=contents,
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
        contents = (
            contents[:MAX_REPORT_CHARACTERS]
            + f"\n\nReport truncated. [Download the untruncated content (ZIP)]({artifact_url(run, artifact)})."
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
    elif full_report is not None:
        report_url = artifact_url(run, full_report)
        contents = (
            f"{contents.rstrip()}\n\n[Download the full report (ZIP)]({report_url})"
        )
    return render_comment(report, contents, run)


def artifact_url(run: dict, artifact: dict) -> str:
    return f"https://github.com/{REPOSITORY}/actions/runs/{run['id']}/artifacts/{artifact['id']}"


def render_comment(report: str, contents: str, run: dict) -> str:
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
    if not is_current_pr_head(pull, run):
        print("The run is not for the current head of this PR.", file=sys.stderr)
        return None
    return run


def fork_pull_request(run: dict) -> dict | None:
    if run["pull_requests"]:
        if len(run["pull_requests"]) == 1:
            return api(f"pulls/{run['pull_requests'][0]['number']}")
        return None

    # GitHub usually omits the PR number on fork runs. A fork branch can target
    # multiple base branches, or be reused for a new PR after an earlier one closed.
    owner = run["head_repository"]["owner"]["login"]
    candidates = (
        pull
        for pull in api_items(
            "pulls", head=f"{owner}:{run['head_branch']}", state="all"
        )
        if (pull["head"]["repo"] or {}).get("id") == run["head_repository"]["id"]
        and pull["head"]["ref"] == run["head_branch"]
        and pull["created_at"] <= run["created_at"]
        and (pull["closed_at"] is None or pull["closed_at"] >= run["created_at"])
    )
    pull = next(candidates, None)
    if next(candidates, None) is not None:
        return None
    return pull


def current_fork_target(
    run_id: int, run_attempt: int, fork_repository_id: int
) -> tuple[dict, int] | None:
    run = api(f"actions/runs/{run_id}")
    if (
        run["repository"]["id"] != REPOSITORY_ID
        or run["event"] != "pull_request"
        or fork_repository_id == REPOSITORY_ID
        or (run["head_repository"] or {}).get("id") != fork_repository_id
        or run["path"] not in FORK_REPORTS
    ):
        raise ValueError("Expected a Ruff report workflow run from the specified fork.")
    if (
        run["run_attempt"] != run_attempt
        or run["status"] != "completed"
        or run["conclusion"] not in {"success", "failure"}
    ):
        return None
    pull = fork_pull_request(run)
    if pull is None or not is_current_pr_head(pull, run):
        print("No unique open PR matches this run's head.", file=sys.stderr)
        return None
    if any(
        event["event"] in {"base_ref_changed", "automatic_base_change_succeeded"}
        and event["created_at"] >= run["created_at"]
        for event in api_items(f"issues/{pull['number']}/events")
    ):
        print("The PR was retargeted after the run was created.", file=sys.stderr)
        return None
    return run, pull["number"]


def is_current_pr_head(pull: dict, run: dict) -> bool:
    return (
        pull["state"] == "open"
        and pull["base"]["repo"]["id"] == REPOSITORY_ID
        and (pull["head"]["repo"] or {}).get("id") == run["head_repository"]["id"]
        and pull["head"]["sha"] == run["head_sha"]
        and pull["head"]["ref"] == run["head_branch"]
    )


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
    update_existing_only: bool = False,
    dry_run: bool = False,
) -> int | None:
    if update_existing_only and stored_comment_id is None:
        return
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
    return publish_comment(
        pull_request,
        report,
        run,
        body,
        existing,
        update_existing_only=update_existing_only,
    )


def publish_fork_links(
    run_id: int, run_attempt: int, fork_repository_id: int, *, dry_run: bool
) -> None:
    target = current_fork_target(run_id, run_attempt, fork_repository_id)
    if target is None:
        return
    run, pull_request = target
    report = FORK_REPORTS[run["path"]]
    links = []
    for name, label in report.artifacts.items():
        # A failed rerun can leave only the previous attempt's artifacts available.
        artifact = find_artifact(run_id, name, since=run["run_started_at"])
        if artifact is not None:
            links.append(f"[{label}]({artifact_url(run, artifact)})")
    if not links:
        print("No report artifacts are available for this attempt.", file=sys.stderr)
        return
    body = render_comment(report.name, " · ".join(links), run)
    if dry_run:
        print(body, end="")
        return

    # A dispatched workflow has no PR-scoped KV namespace to store the comment ID.
    marker = report_marker(report.name)
    existing = next(
        (
            comment
            for comment in api_items(f"issues/{pull_request}/comments")
            if matches_comment(comment, pull_request, marker)
        ),
        None,
    )
    current = current_fork_target(run_id, run_attempt, fork_repository_id)
    if current is not None and current[1] == pull_request:
        publish_comment(pull_request, report.name, run, body, existing)


def publish_comment(
    pull_request: int,
    report: str,
    run: dict,
    body: str,
    existing: dict | None,
    *,
    update_existing_only: bool = False,
) -> int | None:
    if existing and report_run(existing) > (run["id"], run["run_attempt"]):
        print("A newer run already published this report.", file=sys.stderr)
        return existing["id"]
    if existing:
        comment_id = existing["id"]
        if existing["body"] != body:
            api(f"issues/comments/{comment_id}", method="PATCH", body=body)
    elif update_existing_only:
        return
    else:
        comment_id = api(f"issues/{pull_request}/comments", method="POST", body=body)[
            "id"
        ]
    print(f"Published {report} report: {comment_id}")
    return comment_id


def positive_int(value: str) -> int:
    number = int(value)
    if number <= 0:
        raise argparse.ArgumentTypeError("expected a positive integer")
    return number


def main() -> None:
    source_parser = argparse.ArgumentParser(add_help=False)
    source = source_parser.add_mutually_exclusive_group()
    source.add_argument(
        "--workflow-ref", help="Publish the report for a same-repository PR."
    )
    source.add_argument(
        "--fork-repository-id",
        type=positive_int,
        help="Link to artifacts from this fork's report run.",
    )
    # Select the mode before registering its options so argparse can reject
    # same-repository options on fork runs, even if they match their defaults.
    mode, _ = source_parser.parse_known_args()
    source.required = True
    parser = argparse.ArgumentParser(description=__doc__, parents=[source_parser])
    parser.add_argument("--run-id", required=True, type=positive_int)
    parser.add_argument("--run-attempt", required=True, type=positive_int)
    parser.add_argument("--dry-run", action="store_true")
    if mode.fork_repository_id is not None:
        args = parser.parse_args()
        publish_fork_links(
            args.run_id, args.run_attempt, args.fork_repository_id, dry_run=args.dry_run
        )
        return

    report = parser.add_argument_group("same-repository report options")
    report.add_argument("--report", required=True)
    report.add_argument("--pull-request", type=positive_int, required=True)
    report.add_argument("--artifact", required=True)
    report.add_argument("--filename", required=True)
    report.add_argument("--title", default="")
    report.add_argument("--full-report-artifact", default="")
    report.add_argument("--html-bucket", default="")
    report.add_argument("--html-base-url", default="")
    report.add_argument("--html-files", type=read_html_files, default="{}")
    report.add_argument("--comment-id-file", dest="comment_id", type=read_comment_id)
    report.add_argument("--update-existing-only", action="store_true")
    args = parser.parse_args()
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
        update_existing_only=args.update_existing_only,
        dry_run=args.dry_run,
    )
    if comment_id is not None and (output := os.environ.get("GITHUB_OUTPUT")):
        with Path(output).open("a") as file:
            file.write(f"comment-id={comment_id:d}\n")


if __name__ == "__main__":
    main()
