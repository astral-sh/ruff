#!/usr/bin/env -S uv run --script
#
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

"""Collect the exact inputs used by a Ruff ty ecosystem-analyzer run."""

from __future__ import annotations

import argparse
import ast
import json
import re
import subprocess
import sys
import tomllib
from collections.abc import Callable, Sequence
from pathlib import Path
from typing import Any

SHA = r"[0-9a-f]{40}"
ANSI_ESCAPE = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")


class MetadataError(RuntimeError):
    """Raised when historical run metadata is missing or inconsistent."""


CommandRunner = Callable[[Sequence[str]], str]


def run_command(command: Sequence[str]) -> str:
    try:
        result = subprocess.run(
            command,
            check=True,
            capture_output=True,
            text=True,
        )
    except subprocess.CalledProcessError as error:
        detail = error.stderr.strip() or error.stdout.strip()
        raise MetadataError(f"command failed: {' '.join(command)}\n{detail}") from error
    return result.stdout


def payloads(log: str) -> list[str]:
    clean_log = ANSI_ESCAPE.sub("", log).replace("\ufeff", "")
    result = []
    for line in clean_log.splitlines():
        columns = line.split("\t", 2)
        payload = columns[-1]
        if len(columns) == 3:
            payload = re.sub(r"^\S+\s+", "", payload, count=1)
        result.append(payload.strip())
    return result


def unique_value(log: str, pattern: str, label: str) -> str:
    regex = re.compile(pattern)
    values = {
        match.group(1) for line in payloads(log) if (match := regex.fullmatch(line))
    }
    if not values:
        raise MetadataError(f"could not find {label} in the Actions log")
    if len(values) > 1:
        rendered = ", ".join(sorted(values))
        raise MetadataError(f"found conflicting {label} values: {rendered}")
    return values.pop()


def parse_build_log(log: str) -> tuple[str, str]:
    merge_base = unique_value(log, rf"Merge base: ({SHA})", "merge base")
    pr_revision = unique_value(log, rf"PR commit: ({SHA})", "PR revision")
    return merge_base, pr_revision


def parse_shard_log(log: str) -> tuple[str, str, str]:
    exclude_newer = unique_value(
        log,
        r"EXCLUDE_NEWER: (\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z)",
        "EXCLUDE_NEWER",
    )
    analyzer_revision = unique_value(
        log,
        rf"ECOSYSTEM_ANALYZER_COMMIT: ({SHA})",
        "ecosystem-analyzer revision",
    )
    merge_base = unique_value(log, rf"MERGE_BASE: ({SHA})", "shard merge base")
    return exclude_newer, analyzer_revision, merge_base


def parse_minimum_python(source: str) -> tuple[int, int]:
    tree = ast.parse(source)
    for node in tree.body:
        if not isinstance(node, ast.Assign):
            continue
        if not any(
            isinstance(target, ast.Name) and target.id == "MINIMUM_PYTHON_VERSION"
            for target in node.targets
        ):
            continue
        value = ast.literal_eval(node.value)
        if (
            isinstance(value, tuple)
            and len(value) == 2
            and all(isinstance(part, int) for part in value)
        ):
            return value
        break
    raise MetadataError("could not parse ecosystem-analyzer MINIMUM_PYTHON_VERSION")


def parse_mypy_primer_revision(pyproject: str) -> str:
    project = tomllib.loads(pyproject).get("project")
    dependencies = project.get("dependencies", []) if isinstance(project, dict) else []
    revisions: set[str] = set()
    for dependency in dependencies:
        if not isinstance(dependency, str) or not dependency.startswith("mypy-primer "):
            continue
        if match := re.search(
            rf"github\.com/hauntsaninja/mypy_primer(?:\.git)?(?:@|\?rev=)({SHA})",
            dependency,
        ):
            revisions.add(match.group(1))
    if len(revisions) != 1:
        raise MetadataError("expected exactly one pinned mypy-primer revision")
    return revisions.pop()


def literal_keyword(call: ast.Call, name: str) -> Any:
    for keyword in call.keywords:
        if keyword.arg == name:
            return ast.literal_eval(keyword.value)
    return None


def parse_project_versions(
    source: str,
    requested: Sequence[str],
    baseline: tuple[int, int],
) -> dict[str, str]:
    projects: dict[str, tuple[int, int] | None] = {}
    for node in ast.walk(ast.parse(source)):
        if not isinstance(node, ast.Call):
            continue
        if not isinstance(node.func, ast.Name) or node.func.id != "Project":
            continue
        try:
            location = literal_keyword(node, "location")
            name_override = literal_keyword(node, "name_override")
            minimum = literal_keyword(node, "min_python_version")
        except (ValueError, TypeError):
            continue
        if not isinstance(location, str):
            continue
        if name_override is not None and not isinstance(name_override, str):
            raise MetadataError("mypy-primer project name_override is not a string")
        if minimum is not None and (
            not isinstance(minimum, tuple)
            or len(minimum) != 2
            or not all(isinstance(part, int) for part in minimum)
        ):
            raise MetadataError("mypy-primer min_python_version is not a version tuple")
        name = name_override or location.rstrip("/").rsplit("/", 1)[-1]
        if name in projects:
            raise MetadataError(f"duplicate mypy-primer project name: {name}")
        projects[name] = minimum

    missing = sorted(set(requested) - projects.keys())
    if missing:
        raise MetadataError(f"unknown mypy-primer projects: {', '.join(missing)}")

    return {
        name: ".".join(str(part) for part in max(projects[name] or baseline, baseline))
        for name in sorted(set(requested))
    }


def parse_run_reference(value: str) -> tuple[int, int | None]:
    if value.isdigit():
        return int(value), None
    if match := re.search(r"/actions/runs/(\d+)(?:/attempts/(\d+))?(?:/|$)", value):
        parsed_attempt = int(match.group(2)) if match.group(2) is not None else None
        return int(match.group(1)), parsed_attempt
    raise MetadataError(f"invalid Actions run ID or URL: {value}")


def job_by_name(jobs: Sequence[dict[str, Any]], name: str) -> dict[str, Any]:
    matches = [job for job in jobs if job.get("name") == name]
    if len(matches) != 1:
        raise MetadataError(f"expected exactly one {name!r} job")
    return matches[0]


def first_shard_job(jobs: Sequence[dict[str, Any]]) -> dict[str, Any]:
    matches: list[tuple[int, dict[str, Any]]] = []
    for job in jobs:
        if match := re.fullmatch(r"analyze-shards \((\d+)\)", str(job.get("name"))):
            matches.append((int(match.group(1)), job))
    if not matches:
        raise MetadataError("could not find an analyze-shards job")
    return min(matches, key=lambda item: item[0])[1]


def collect_metadata(
    run: str,
    projects: Sequence[str],
    *,
    repo: str,
    analyzer_repo: str,
    attempt: int | None,
    runner: CommandRunner = run_command,
) -> dict[str, Any]:
    run_id, url_attempt = parse_run_reference(run)
    if attempt is not None and url_attempt is not None and attempt != url_attempt:
        raise MetadataError(
            f"Actions URL specifies attempt {url_attempt}, but --attempt specifies {attempt}"
        )
    requested_attempt = attempt if attempt is not None else url_attempt
    view_command = [
        "gh",
        "run",
        "view",
        str(run_id),
        "--repo",
        repo,
        "--json",
        "attempt,databaseId,url,workflowName,status,conclusion,jobs",
    ]
    if requested_attempt is not None:
        view_command.extend(["--attempt", str(requested_attempt)])
    run_data = json.loads(runner(view_command))
    selected_attempt = int(run_data["attempt"])
    if run_data.get("workflowName") != "ty ecosystem-analyzer":
        raise MetadataError("the run is not from the 'ty ecosystem-analyzer' workflow")
    if run_data.get("status") != "completed":
        raise MetadataError("the Actions run has not completed")

    jobs = run_data.get("jobs")
    if not isinstance(jobs, list):
        raise MetadataError("the Actions run did not include job metadata")
    build_job = job_by_name(jobs, "Build ty")
    shard_job = first_shard_job(jobs)

    def job_log(job: dict[str, Any]) -> str:
        return runner(
            [
                "gh",
                "run",
                "view",
                str(run_id),
                "--repo",
                repo,
                "--attempt",
                str(selected_attempt),
                "--job",
                str(job["databaseId"]),
                "--log",
            ]
        )

    merge_base, pr_revision = parse_build_log(job_log(build_job))
    exclude_newer, analyzer_revision, shard_merge_base = parse_shard_log(
        job_log(shard_job)
    )
    if shard_merge_base != merge_base:
        raise MetadataError("build and shard logs disagree on the merge base")

    def repository_file(repository: str, path: str, revision: str) -> str:
        return runner(
            [
                "gh",
                "api",
                "--method",
                "GET",
                f"repos/{repository}/contents/{path}",
                "-f",
                f"ref={revision}",
                "-H",
                "Accept: application/vnd.github.raw+json",
            ]
        )

    analyzer_pyproject = repository_file(
        analyzer_repo, "pyproject.toml", analyzer_revision
    )
    analyzer_config = repository_file(
        analyzer_repo,
        "src/ecosystem_analyzer/config.py",
        analyzer_revision,
    )
    minimum_python = parse_minimum_python(analyzer_config)
    primer_revision = parse_mypy_primer_revision(analyzer_pyproject)
    primer_projects = repository_file(
        "hauntsaninja/mypy_primer",
        "mypy_primer/projects.py",
        primer_revision,
    )

    return {
        "run": {
            "attempt": selected_attempt,
            "conclusion": run_data["conclusion"],
            "id": int(run_data["databaseId"]),
            "url": run_data["url"],
        },
        "ruff": {
            "merge_base": merge_base,
            "pr_revision": pr_revision,
        },
        "exclude_newer": exclude_newer,
        "ecosystem_analyzer": {
            "minimum_python": ".".join(str(part) for part in minimum_python),
            "revision": analyzer_revision,
        },
        "mypy_primer": {"revision": primer_revision},
        "project_python": parse_project_versions(
            primer_projects, projects, minimum_python
        ),
    }


def render_metadata(metadata: dict[str, Any]) -> str:
    return json.dumps(metadata, indent=2, sort_keys=True) + "\n"


def write_metadata(metadata: dict[str, Any], output: Path) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(render_metadata(metadata))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", help="Actions run ID or URL")
    parser.add_argument("projects", nargs="*", help="mypy-primer project names")
    parser.add_argument("--repo", default="astral-sh/ruff")
    parser.add_argument("--analyzer-repo", default="astral-sh/ecosystem-analyzer")
    parser.add_argument("--attempt", type=int)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()

    try:
        metadata = collect_metadata(
            args.run,
            args.projects,
            repo=args.repo,
            analyzer_repo=args.analyzer_repo,
            attempt=args.attempt,
        )
    except (MetadataError, json.JSONDecodeError, KeyError, TypeError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error

    if args.output is None:
        print(render_metadata(metadata), end="")
    else:
        write_metadata(metadata, args.output)


if __name__ == "__main__":
    main()
