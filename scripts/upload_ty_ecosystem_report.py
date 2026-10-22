# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Upload ty ecosystem HTML reports from a same-repository pull request run."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path

from publish_pr_report import (
    REPOSITORY,
    current_run,
    download_artifact,
    find_artifact,
    read_report,
)

MAX_ARCHIVE_BYTES = 128 * 1024 * 1024
MAX_REPORT_BYTES = 128 * 1024 * 1024
REPORT_FILES = {
    "diff.html": "text/html; charset=utf-8",
    "timing.html": "text/html; charset=utf-8",
    "diff.json": "application/json",
}


def upload(
    run_id: int,
    run_attempt: int,
    pull_request: int,
    *,
    workflow_ref: str,
    bucket: str,
    base_url: str,
) -> str | None:
    expected_workflow = (
        f"{REPOSITORY}/.github/workflows/ty-ecosystem-analyzer.yaml"
        f"@refs/pull/{pull_request}/merge"
    )
    if workflow_ref != expected_workflow:
        raise ValueError("Only ty ecosystem-analyzer runs can upload reports.")
    if not current_run(run_id, run_attempt, workflow_ref, pull_request):
        return None
    artifact = find_artifact(run_id, "full-report")
    if artifact is None:
        print("No full-report artifact is available.", file=sys.stderr)
        return None
    archive = download_artifact(artifact, max_bytes=MAX_ARCHIVE_BYTES)
    reports = {
        filename: read_report(archive, filename, max_bytes=MAX_REPORT_BYTES).encode()
        for filename in REPORT_FILES
    }
    if not current_run(run_id, run_attempt, workflow_ref, pull_request):
        return None
    prefix = f"pull-requests/{pull_request}/{run_id}/{run_attempt}"
    for filename, contents in reports.items():
        subprocess.run(
            [
                "aws",
                "s3",
                "cp",
                "-",
                f"s3://{bucket}/{prefix}/{filename}",
                "--content-type",
                REPORT_FILES[filename],
                "--cache-control",
                "public, max-age=3600",
                "--only-show-errors",
            ],
            input=contents,
            check=True,
            timeout=120,
        )
    return f"{base_url.rstrip('/')}/{prefix}"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-id", required=True, type=int)
    parser.add_argument("--run-attempt", required=True, type=int)
    parser.add_argument("--pull-request", required=True, type=int)
    parser.add_argument("--workflow-ref", required=True)
    parser.add_argument("--bucket", required=True)
    parser.add_argument("--base-url", required=True)
    args = parser.parse_args()
    if min(args.run_id, args.run_attempt, args.pull_request) <= 0:
        parser.error("The run ID, attempt, and pull request number must be positive.")
    if not args.bucket or not args.base_url:
        parser.error("The report bucket and public base URL must be configured.")
    report_url = upload(
        args.run_id,
        args.run_attempt,
        args.pull_request,
        workflow_ref=args.workflow_ref,
        bucket=args.bucket,
        base_url=args.base_url,
    )
    if report_url is not None and (output := os.environ.get("GITHUB_OUTPUT")):
        with Path(output).open("a") as file:
            file.write(f"report-url={report_url}\n")


if __name__ == "__main__":
    main()
