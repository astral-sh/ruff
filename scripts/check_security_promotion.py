# /// script
# requires-python = ">=3.12"
# dependencies = ["pyyaml>=6,<7"]
# ///
"""Exercise the promotion workflow with local Git repositories and a fake GitHub API.

Run with `uv run scripts/check_security_promotion.py`. No GitHub credentials or
network access are used by the scenarios.
"""

from __future__ import annotations

import copy
import json
import os
import shlex
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]
SOURCE_ID = 1373465128
PUBLIC_ID = 523043277
SOURCE = "astral-sh/ruff-security"
PUBLIC = "astral-sh/ruff"


def mock_github(args: list[str]) -> None:
    """Fail closed on any API call not explicitly modeled by a scenario."""
    state = json.loads(Path(os.environ["MOCK_STATE"]).read_text())
    with Path(os.environ["MOCK_CALLS"]).open("a") as calls:
        calls.write(json.dumps(args) + "\n")
    source = state["source"]
    head_ref = source["head"]["ref"]
    if args[:2] == ["pr", "view"]:
        if "--jq" in args:
            assert args[2] == "123"
            print(state.get("public_head", source["head"]["sha"]))
            return
        result = {
            "state": source["state"].upper(),
            "isDraft": source["draft"],
            "labels": source["labels"],
            "baseRefName": source["base"]["ref"],
            "headRefName": source["head"]["ref"],
            "headRefOid": source["head"]["sha"],
            "isCrossRepository": source["head"]["repo"]["id"] != SOURCE_ID,
        }
    elif args[:2] == ["pr", "list"]:
        repository = args[args.index("--repo") + 1]
        status = args[args.index("--state") + 1]
        if (repository == SOURCE and status == "open") or (
            repository == PUBLIC and status == "open"
        ):
            result = []
        elif repository == PUBLIC and status == "merged":
            result = state["merged_parents"]
        else:
            raise AssertionError(args)
    elif args[0] == "api":
        endpoint = next(arg for arg in args if arg.startswith("repos/"))
        method = args[args.index("--method") + 1]
        if method == "GET":
            if endpoint == f"repos/{SOURCE}/pulls/10":
                result = source
            elif endpoint.endswith("/events"):
                result = [state["events"]]
            elif endpoint.endswith("/permission"):
                result = {"permission": state.get("permission", "write")}
            elif endpoint == f"repos/{PUBLIC}/git/matching-refs/heads/main":
                result = [{"ref": "refs/heads/main"}]
            elif endpoint == f"repos/{PUBLIC}/pulls":
                result = state.get("existing_prs", [])
            elif endpoint == f"repos/{PUBLIC}/git/matching-refs/heads/parent":
                result = []
            elif endpoint == f"repos/{PUBLIC}/git/matching-refs/heads/{head_ref}":
                result = state.get("existing_refs", [])
            elif endpoint == f"repos/{PUBLIC}/pulls/20":
                result = state["merged_parent"]
            elif endpoint == f"repos/{SOURCE}/git/ref/heads/main":
                assert args[-2:] == ["--jq", ".object.sha"]
                print(state["parent_base_sha"])
                return
            elif endpoint.startswith(f"repos/{SOURCE}/compare/"):
                assert endpoint.endswith(
                    f"/{state['merged_parent']['merge_commit_sha']}...{state['parent_base_sha']}"
                )
                result = {"status": state["comparison_status"]}
            elif endpoint.endswith("/comments"):
                result = []
            else:
                raise AssertionError(args)
        elif method == "POST" and endpoint == f"repos/{PUBLIC}/pulls":
            payload = json.load(sys.stdin)
            assert payload["base"] == "main" and payload["head"] == head_ref
            assert payload["body"] == "Fixes astral-sh/ruff-security#42."
            result = {"number": 123}
        elif method == "POST" and endpoint.endswith("/labels"):
            assert json.load(sys.stdin) == {"labels": ["ty"]}
            result = {}
        elif (method == "POST" and endpoint.endswith("/assignees")) or (
            method == "DELETE" and endpoint.endswith("/labels/bot:promote")
        ):
            result = {}
        else:
            raise AssertionError(args)
    elif args[:2] in (["pr", "comment"], ["pr", "close"]):
        result = {}
    else:
        raise AssertionError(args)
    print(json.dumps(result))


def git(directory: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(directory), *args],
        check=True,
        text=True,
        capture_output=True,
    )
    return result.stdout.strip()


def main() -> None:
    workflow = yaml.safe_load(
        (ROOT / ".github/workflows/promote-pull-request.yml").read_text()
    )
    scripts = {
        (job_id, step["id"]): step["run"]
        for job_id, job in workflow["jobs"].items()
        for step in job.get("steps", [])
        if "run" in step
    }
    parent_workflow = yaml.safe_load(
        (ROOT / ".github/workflows/update-pull-request-parent.yml").read_text()
    )
    scripts["parent", "update"] = next(
        step["run"]
        for step in parent_workflow["jobs"]["update"]["steps"]
        if step.get("id") == "update"
    )
    for script in scripts.values():
        subprocess.run(["bash", "-n"], input=script, text=True, check=True)

    with tempfile.TemporaryDirectory(prefix="ruff-promotion-") as directory:
        temp = Path(directory)
        public = temp / "public.git"
        private = temp / "private.git"
        candidate = temp / "candidate"
        candidate.mkdir()
        git(candidate, "init", "--initial-branch=main")
        git(candidate, "config", "user.name", "Promotion fixture")
        git(candidate, "config", "user.email", "fixture@example.com")
        (candidate / "public.txt").write_text("public\n")
        git(candidate, "add", ".")
        git(candidate, "commit", "-m", "Public base")
        base = git(candidate, "rev-parse", "HEAD")
        git(temp, "clone", "--bare", str(candidate), str(public))
        git(candidate, "checkout", "-b", "zsol/fix")
        (candidate / "fix.txt").write_text("approved fix\n")
        git(candidate, "add", ".")
        git(candidate, "commit", "-m", "Approved fix")
        head = git(candidate, "rev-parse", "HEAD")
        git(candidate, "checkout", "-b", "unrelated", base)
        (candidate / "unrelated.txt").write_text("unapproved private work\n")
        git(candidate, "add", ".")
        git(candidate, "commit", "-m", "Unrelated private change")
        unrelated = git(candidate, "rev-parse", "HEAD")
        git(temp, "clone", "--bare", str(candidate), str(private))
        git(candidate, "checkout", "main")
        config = temp / "gitconfig"
        config.write_text(
            f'[url "{public}"]\n\tinsteadOf = https://github.com/{PUBLIC}.git\n'
            f'[url "{private}"]\n\tinsteadOf = https://github.com/{SOURCE}.git\n'
        )
        state = {
            "source": {
                "state": "open",
                "draft": True,
                "title": "Fix a bug",
                "body": "Fixes #42.",
                "labels": [
                    {"name": name}
                    for name in ("bot:promote", "priority:high", "risk:low", "ty")
                ],
                "base": {"ref": "main", "sha": base, "repo": {"id": SOURCE_ID}},
                "head": {"ref": "zsol/fix", "sha": head, "repo": {"id": SOURCE_ID}},
            },
            "events": [
                {
                    "id": 7,
                    "event": "labeled",
                    "label": {"name": "bot:promote"},
                    "actor": {"login": "maintainer", "type": "User"},
                }
            ],
        }
        common_env = {
            **os.environ,
            "GITHUB_REPOSITORY": SOURCE,
            "GITHUB_REPOSITORY_ID": str(SOURCE_ID),
            "RUFF_REPOSITORY_ID": str(PUBLIC_ID),
            "AUTOMATIONS_BOT_ID": "305554984",
            "PROMOTION_LABEL": "bot:promote",
            "BASE_REF": "main",
            "BASE_SHA": base,
            "HEAD_REF": "zsol/fix",
            "EXPECTED_HEAD_SHA": head,
            "PULL_REQUEST_NUMBER": "10",
            "APPROVAL_ID": "7",
            "PROMOTER": "maintainer",
            "PROMOTED_HEAD": head,
            "SOURCE_BASE_REF": "main",
            "SOURCE_BASE_SHA": base,
            "ACTION": "promote",
            "GIT_CONFIG_GLOBAL": str(config),
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_COUNT": "0",
            "GIT_ALLOW_PROTOCOL": "file",
            "GH_TOKEN": "unused-fixture-token",
            "REBASE_REF": "",
            "REJECTION_REASON": "The approved commit changed.",
            "CHANGED_HEAD": "",
            "UPSTREAM_NUMBER": "",
            "GITHUB_RUN_ID": "1",
        }
        count = 0

        def run(
            name: str,
            job: str,
            step: str,
            data: dict,
            expected: str,
            *,
            failure: bool = False,
            cwd: Path = candidate,
            bundle: Path | None = None,
        ) -> list[list[str]]:
            nonlocal count
            run_dir = temp / name
            run_dir.mkdir()
            if bundle is not None:
                (run_dir / "updated-parent").mkdir()
                shutil.copyfile(
                    bundle, run_dir / "updated-parent/updated-parent.bundle"
                )
            state_path = run_dir / "state.json"
            state_path.write_text(json.dumps(data))
            output = run_dir / "output"
            output.touch()
            calls = run_dir / "calls"
            calls.touch()
            env = {
                **common_env,
                "RUNNER_TEMP": str(run_dir),
                "GITHUB_OUTPUT": str(output),
                "GITHUB_STEP_SUMMARY": str(run_dir / "summary"),
                "MOCK_STATE": str(state_path),
                "MOCK_CALLS": str(calls),
            }
            # Intercept every GitHub invocation, including credentials helpers.
            prefix = (
                f"gh() {{ {shlex.quote(sys.executable)} "
                f'{shlex.quote(str(Path(__file__).resolve()))} --mock-gh "$@"; }}\n'
                "export -f gh\n"
            )
            script = scripts[job, step]
            result = subprocess.run(
                [
                    "bash",
                    "--noprofile",
                    "--norc",
                    "-euo",
                    "pipefail",
                    "-c",
                    prefix + script,
                ],
                cwd=cwd,
                env=env,
                text=True,
                capture_output=True,
                check=False,
                timeout=30,
            )
            evidence = result.stdout + result.stderr + output.read_text()
            assert (result.returncode != 0) == failure, (name, evidence)
            assert expected in evidence, (name, evidence)
            count += 1
            print(f"PASS {name}")
            return [json.loads(line) for line in calls.read_text().splitlines()]

        run("draft-label", "prepare", "verify", state, "head_ref=zsol/fix")
        run("plan", "prepare", "plan", state, "action=promote")
        for name, edit, expected in [
            (
                "missing-label",
                lambda s: s["source"].update(labels=[]),
                "requires the bot:promote label",
            ),
            (
                "reader",
                lambda s: s.update(permission="read"),
                "not approved by a repository writer",
            ),
            (
                "bot-label",
                lambda s: s["events"][0]["actor"].update(type="Bot"),
                "no current human promotion event",
            ),
            (
                "ready-only",
                lambda s: s["events"][0].update(event="ready_for_review"),
                "no current human promotion event",
            ),
            (
                "fork",
                lambda s: s["source"]["head"]["repo"].update(id=1),
                "no longer eligible",
            ),
        ]:
            data = copy.deepcopy(state)
            edit(data)
            run(name, "prepare", "plan", data, expected, failure=True)
        data = copy.deepcopy(state)
        data["source"]["head"]["sha"] = "a" * 40
        run("changed-head", "prepare", "plan", data, "changed_head=" + "a" * 40)
        for event in ("unlabeled", "labeled"):
            data = copy.deepcopy(state)
            data["events"].append({**data["events"][0], "id": 8, "event": event})
            run(
                f"superseded-{event}",
                "promote",
                "promote",
                data,
                "approval changed",
                failure=True,
            )
        data = copy.deepcopy(state)
        data["permission"] = "read"
        run(
            "revoked-writer",
            "promote",
            "promote",
            data,
            "no longer approved",
            failure=True,
        )
        data = copy.deepcopy(state)
        data["source"]["base"]["sha"] = "a" * 40
        run("changed-base", "promote", "promote", data, "base changed", failure=True)
        data = copy.deepcopy(state)
        data["existing_refs"] = [
            {"ref": "refs/heads/zsol/fix", "object": {"sha": "a" * 40}}
        ]
        run(
            "branch-collision",
            "promote",
            "promote",
            data,
            "already exists",
            failure=True,
        )
        # Use a valid private commit to prove the public-base check rejects an
        # unpublished parent before it can transfer any private objects.
        common_env["SOURCE_BASE_SHA"] = unrelated
        data = copy.deepcopy(state)
        data["source"]["base"]["sha"] = unrelated
        run(
            "unpublished-parent",
            "promote",
            "promote",
            data,
            "not on its upstream branch",
            failure=True,
        )
        common_env["SOURCE_BASE_SHA"] = base
        run("publish-approved", "promote", "promote", state, "number=123")
        assert git(public, "rev-parse", "refs/heads/zsol/fix") == head
        assert (
            git(public, "for-each-ref", "--format=%(refname)")
            == "refs/heads/main\nrefs/heads/zsol/fix"
        )
        assert (
            subprocess.run(
                ["git", "-C", str(public), "cat-file", "-e", unrelated],
                capture_output=True,
                check=False,
            ).returncode
            != 0
        )
        data = copy.deepcopy(state)
        data["existing_prs"] = [
            {
                "number": 123,
                "state": "open",
                "base": {"ref": "main", "repo": {"id": PUBLIC_ID}},
                "head": {"ref": "zsol/fix", "sha": head, "repo": {"id": PUBLIC_ID}},
            }
        ]
        calls = run("reuse-public-pr", "promote", "promote", data, "number=123")
        assert not any(
            "POST" in call and f"repos/{PUBLIC}/pulls" in call for call in calls
        )
        calls = run("recover", "recover", "recover", state, "")
        assert any("DELETE" in call for call in calls)
        common_env["UPSTREAM_NUMBER"] = "123"
        calls = run("close-source", "promote", "close", state, "")
        assert any(call[:2] == ["pr", "close"] for call in calls)
        data = copy.deepcopy(state)
        data["public_head"] = "a" * 40
        calls = run("moved-public-head", "promote", "close", data, "rejection_reason=")
        assert not any(call[:2] == ["pr", "close"] for call in calls)

        for outcome in ("clean", "conflicts", "empty"):
            fixture = temp / f"parent-{outcome}"
            git(temp, "clone", str(private), str(fixture))
            git(fixture, "config", "user.name", "Promotion fixture")
            git(fixture, "config", "user.email", "fixture@example.com")
            git(fixture, "checkout", "-b", "parent", base)
            (fixture / "parent.txt").write_text("parent\n")
            git(fixture, "add", ".")
            git(fixture, "commit", "-m", "Parent change")
            parent = git(fixture, "rev-parse", "HEAD")
            git(fixture, "checkout", "-b", "child")
            (fixture / "parent.txt").write_text("child\n")
            git(fixture, "add", ".")
            git(fixture, "commit", "-m", "Child change")
            child = git(fixture, "rev-parse", "HEAD")
            git(fixture, "push", "--force", "origin", "child")
            git(fixture, "checkout", "-B", "main", base)
            git(fixture, "merge", "--squash", "parent")
            git(fixture, "commit", "-m", "Merged parent")
            merged = git(fixture, "rev-parse", "HEAD")
            if outcome != "clean":
                (fixture / "parent.txt").write_text(
                    "child\n" if outcome == "empty" else "conflicting public change\n"
                )
                git(fixture, "add", ".")
                git(fixture, "commit", "-m", "Public change")
            destination = git(fixture, "rev-parse", "HEAD")
            git(fixture, "update-ref", "refs/remotes/origin/main", destination)
            common_env.update(
                BASE_SHA=destination,
                BASE_PREVIOUS_SHA=parent,
                PARENT_MERGE_SHA=merged,
                HEAD_REF="child",
                HEAD_SHA=child,
                GIT_EDITOR="true",
            )
            stacked = copy.deepcopy(state)
            stacked["source"]["base"].update(ref="parent", sha=parent)
            stacked["source"]["head"].update(ref="child", sha=child)
            if outcome == "clean":
                stacked.update(
                    merged_parents=[
                        {
                            "number": 20,
                            "headRefName": "parent",
                            "headRepository": {"nameWithOwner": PUBLIC},
                        }
                    ],
                    merged_parent={
                        "merged_at": "2026-09-24T00:00:00Z",
                        "merge_commit_sha": merged,
                        "base": {"ref": "main", "repo": {"id": PUBLIC_ID}},
                        "head": {
                            "ref": "parent",
                            "sha": parent,
                            "repo": {"id": PUBLIC_ID},
                        },
                    },
                    parent_base_sha=destination,
                    comparison_status="identical",
                )
                common_env.update(BASE_REF="parent", EXPECTED_HEAD_SHA=child)
                run(
                    "merged-parent-plan",
                    "prepare",
                    "plan",
                    stacked,
                    "action=update-parent",
                    cwd=fixture,
                )
                common_env["BASE_REF"] = "main"
            run(
                f"parent-update-{outcome}",
                "parent",
                "update",
                state,
                f"outcome={outcome}",
                cwd=fixture,
            )
            assert git(private, "rev-parse", "refs/heads/child") == child
            bundle = temp / f"parent-update-{outcome}" / "updated-parent.bundle"
            assert bundle.exists() == (outcome == "clean")
            if outcome == "clean":
                git(fixture, "bundle", "verify", str(bundle))
                assert (fixture / "parent.txt").read_text() == "child\n"
                rebased = git(fixture, "rev-parse", "HEAD")
                # A separate consumer has the synced base, but not the rebased
                # child until the verification step imports the producer's bundle.
                git(fixture, "push", str(public), f"{destination}:refs/heads/main")
                consumer = temp / "consumer"
                git(temp, "clone", str(public), str(consumer))
                common_env.update(
                    ACTION="update-parent",
                    UPDATED_HEAD=rebased,
                    SOURCE_BASE_REF="parent",
                    SOURCE_BASE_SHA=parent,
                    PROMOTED_HEAD=rebased,
                )
                run(
                    "verify-parent-bundle",
                    "promote",
                    "verify",
                    stacked,
                    f"head_sha={rebased}",
                    cwd=consumer,
                    bundle=bundle,
                )
                run(
                    "publish-rebased-child",
                    "promote",
                    "promote",
                    stacked,
                    "number=123",
                    cwd=consumer,
                )
                assert git(public, "rev-parse", "refs/heads/child") == rebased
                assert git(private, "rev-parse", "refs/heads/child") == child
        print(f"Passed {count} promotion scenarios.")


if __name__ == "__main__":
    if sys.argv[1:] and sys.argv[1] == "--mock-gh":
        mock_github(sys.argv[2:])
    else:
        main()
