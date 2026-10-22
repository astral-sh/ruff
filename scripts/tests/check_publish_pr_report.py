# /// script
# requires-python = ">=3.10"
# dependencies = []
#
# [tool.ty.environment]
# root = [".."]
# ///
from __future__ import annotations

import contextlib
import io
import os
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from typing import Any
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import publish_pr_report as publisher


def archive(members):
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w", compression=zipfile.ZIP_DEFLATED) as handle:
        for name, data in members:
            handle.writestr(name, data)
    return buffer.getvalue()


class PublisherScenarios(unittest.TestCase):
    def setUp(self):
        self.workflow_run: dict[str, Any] = {
            "id": 100,
            "run_attempt": 2,
            "head_sha": "a" * 40,
            "status": "completed",
            "conclusion": "failure",
            "repository": {"id": publisher.comments.REPOSITORY_ID},
            "head_repository": {"id": publisher.comments.REPOSITORY_ID},
            "event": "pull_request",
            "path": ".github/workflows/ci.yaml",
            "head_branch": "feature",
        }
        self.pull: dict[str, Any] = {
            "number": 42,
            "state": "open",
            "base": {"repo": {"id": publisher.comments.REPOSITORY_ID}},
            "head": {
                "repo": {"id": publisher.comments.REPOSITORY_ID},
                "sha": "a" * 40,
                "ref": "feature",
            },
        }
        self.artifact = {
            "id": 200,
            "size_in_bytes": 100,
            "name": "ecosystem-result",
            "expired": False,
        }

    def reads_only_fixed_member_without_extracting(self):
        data = archive(
            [("../../publisher.py", "malicious"), ("ecosystem-result", "result")]
        )
        self.assertEqual(publisher.read_report(data, "ecosystem-result"), "result")

    def missing_member_is_rejected(self):
        with self.assertRaises(ValueError):
            publisher.read_report(
                archive([("elsewhere", "result")]), "ecosystem-result"
            )

    def duplicate_member_is_rejected(self):
        with self.assertWarns(UserWarning):
            data = archive(
                [("ecosystem-result", "first"), ("ecosystem-result", "second")]
            )
        with self.assertRaises(ValueError):
            publisher.read_report(data, "ecosystem-result")

    def uncompressed_size_is_bounded(self):
        data = archive([("ecosystem-result", "x" * 101)])
        with (
            patch.object(publisher, "MAX_REPORT_BYTES", 100),
            self.assertRaises(ValueError),
        ):
            publisher.read_report(data, "ecosystem-result")

    def large_archive_is_rejected_before_download(self):
        with (
            patch.object(publisher.subprocess, "run") as download,
            self.assertRaises(ValueError),
        ):
            publisher.download_artifact(
                {"id": 1, "size_in_bytes": publisher.MAX_ARCHIVE_BYTES + 1}
            )
        download.assert_not_called()

    def selects_latest_unexpired_artifact(self):
        old = dict(self.artifact, id=10)
        expired = dict(self.artifact, id=300, expired=True)
        with patch.object(
            publisher.comments,
            "api",
            return_value={"artifacts": [old, self.artifact, expired]},
        ):
            self.assertEqual(
                publisher.find_artifact(100, "ecosystem-result"), self.artifact
            )

    def failed_source_can_publish(self):
        with patch.object(
            publisher.comments, "api", side_effect=[self.workflow_run, self.pull]
        ):
            self.assertEqual(
                publisher.current_run(100, 2, "ruff-ecosystem", 42),
                (self.workflow_run, self.pull),
            )

    def new_attempt_invalidates_old_completion(self):
        with patch.object(publisher.comments, "api", return_value=self.workflow_run):
            self.assertIsNone(publisher.current_run(100, 1, "ruff-ecosystem", 42))

    def can_publish_while_other_ci_jobs_run(self):
        self.workflow_run["status"] = "in_progress"
        with patch.object(
            publisher.comments, "api", side_effect=[self.workflow_run, self.pull]
        ):
            self.assertIsNotNone(publisher.current_run(100, 2, "ruff-ecosystem", 42))

    def rejects_fork_source(self):
        self.workflow_run["head_repository"]["id"] = 99
        with (
            patch.object(publisher.comments, "api", return_value=self.workflow_run),
            self.assertRaises(ValueError),
        ):
            publisher.current_run(100, 2, "ruff-ecosystem", 42)

    def rejects_wrong_producer(self):
        self.workflow_run["path"] = ".github/workflows/other.yaml"
        with (
            patch.object(publisher.comments, "api", return_value=self.workflow_run),
            self.assertRaises(ValueError),
        ):
            publisher.current_run(100, 2, "ruff-ecosystem", 42)

    def rejects_stale_pr_head(self):
        self.pull["head"]["sha"] = "b" * 40
        with patch.object(
            publisher.comments, "api", side_effect=[self.workflow_run, self.pull]
        ):
            self.assertIsNone(publisher.current_run(100, 2, "ruff-ecosystem", 42))

    def render_keeps_metadata_and_truncation_link(self):
        body = publisher.render_report(
            "ruff-ecosystem", "x" * 70000, self.workflow_run, self.artifact
        )
        self.assertLess(len(body), 65536)
        self.assertTrue(
            body.startswith(publisher.comments.REPORTS["ruff-ecosystem"].marker + "\n")
        )
        self.assertEqual(publisher.report_run({"body": body}), (100, 2))
        self.assertIn("Download the full report", body)
        self.assertIn("/attempts/2", body)

    def publishing(self, existing=None, sources=None, contents="result", dry_run=False):
        api = Mock(return_value={"id": 900})
        lookup = Mock(return_value=existing)
        with contextlib.ExitStack() as stack:
            stack.enter_context(
                patch.object(
                    publisher,
                    "current_run",
                    side_effect=sources,
                    return_value=(self.workflow_run, {"number": 42}),
                )
            )
            stack.enter_context(
                patch.object(publisher, "find_artifact", return_value=self.artifact)
            )
            stack.enter_context(
                patch.object(
                    publisher,
                    "download_artifact",
                    return_value=archive([("ecosystem-result", contents)]),
                )
            )
            stack.enter_context(patch.object(publisher.comments, "find_report", lookup))
            stack.enter_context(patch.object(publisher.comments, "api", api))
            stack.enter_context(contextlib.redirect_stdout(io.StringIO()))
            comment_id = publisher.publish(
                100, 2, "ruff-ecosystem", 42, stored_comment_id=5, dry_run=dry_run
            )
        if lookup.called:
            lookup.assert_called_once_with(42, "ruff-ecosystem", 5)
        return api, comment_id

    def returns_creation_response_id(self):
        api, comment_id = self.publishing()
        self.assertEqual(api.call_args.kwargs["method"], "POST")
        self.assertEqual(comment_id, 900)

    def updates_stored_comment(self):
        api, comment_id = self.publishing(existing={"id": 5, "body": "old"})
        self.assertEqual(api.call_args.args[0], "issues/comments/5")
        self.assertEqual(api.call_args.kwargs["method"], "PATCH")
        self.assertEqual(comment_id, 5)

    def unchanged_report_returns_stored_id(self):
        existing = {
            "id": 5,
            "body": publisher.render_report(
                "ruff-ecosystem", "result", self.workflow_run, self.artifact
            ),
        }
        api, comment_id = self.publishing(existing=existing)
        api.assert_not_called()
        self.assertEqual(comment_id, 5)

    def newer_published_run_wins(self):
        api, comment_id = self.publishing(
            existing={"id": 5, "body": "marker\n<!-- report-run:101:1 -->\nnewer"}
        )
        api.assert_not_called()
        self.assertEqual(comment_id, 5)

    def changed_head_after_download_prevents_writes(self):
        api, comment_id = self.publishing(
            sources=[(self.workflow_run, {"number": 42}), None]
        )
        api.assert_not_called()
        self.assertIsNone(comment_id)

    def changed_pr_after_download_prevents_writes(self):
        api, comment_id = self.publishing(
            sources=[
                (self.workflow_run, {"number": 42}),
                (self.workflow_run, {"number": 43}),
            ]
        )
        api.assert_not_called()
        self.assertIsNone(comment_id)

    def dry_run_does_not_write(self):
        api, comment_id = self.publishing(dry_run=True)
        api.assert_not_called()
        self.assertIsNone(comment_id)

    def empty_report_does_not_write(self):
        api, comment_id = self.publishing(contents="  \n")
        api.assert_not_called()
        self.assertIsNone(comment_id)

    def workflow_output_is_only_written_for_a_comment(self):
        for comment_id in (None, 900):
            with (
                self.subTest(comment_id=comment_id),
                tempfile.TemporaryDirectory() as directory,
            ):
                output = Path(directory) / "output"
                argv = [
                    "publish_pr_report.py",
                    "--run-id",
                    "100",
                    "--run-attempt",
                    "2",
                    "--report",
                    "ruff-ecosystem",
                    "--pull-request",
                    "42",
                ]
                with (
                    patch.object(sys, "argv", argv),
                    patch.dict(os.environ, {"GITHUB_OUTPUT": str(output)}),
                    patch.object(
                        publisher, "publish", return_value=comment_id
                    ) as publish,
                ):
                    publisher.main()
                publish.assert_called_once_with(
                    100, 2, "ruff-ecosystem", 42, stored_comment_id=None, dry_run=False
                )
                if comment_id is None:
                    self.assertFalse(output.exists())
                else:
                    self.assertEqual(output.read_text(), "comment-id=900\n")

    def malformed_stored_id_prevents_publication(self):
        for value in ("", "invalid", "0", "23\n"):
            argv = [
                "publish_pr_report.py",
                "--run-id",
                "100",
                "--run-attempt",
                "2",
                "--report",
                "ruff-ecosystem",
                "--pull-request",
                "42",
                "--comment-id",
                value,
            ]
            with (
                self.subTest(value=value),
                patch.object(sys, "argv", argv),
                patch.object(publisher, "publish") as publish,
                contextlib.redirect_stderr(io.StringIO()),
                self.assertRaises(SystemExit),
            ):
                publisher.main()
            publish.assert_not_called()

    def truncated_summary_keeps_full_html_link(self):
        body = publisher.render_report(
            "ty-ecosystem",
            "x" * 70000,
            self.workflow_run,
            self.artifact,
            full_report={"id": 300},
        )
        self.assertLess(len(body), 65536)
        self.assertIn("/runs/100/artifacts/300", body)
        self.assertIn("Download the HTML reports (ZIP)", body)

    def ty_links_full_report_without_downloading_it(self):
        for full_report in ({"id": 300}, None):
            with self.subTest(full_report=full_report), contextlib.ExitStack() as stack:
                stack.enter_context(
                    patch.object(
                        publisher,
                        "current_run",
                        return_value=(self.workflow_run, {"number": 42}),
                    )
                )
                find = stack.enter_context(
                    patch.object(
                        publisher,
                        "find_artifact",
                        side_effect=[self.artifact, full_report],
                    )
                )
                download = stack.enter_context(
                    patch.object(
                        publisher,
                        "download_artifact",
                        return_value=archive([("comment.md", "summary")]),
                    )
                )
                api = stack.enter_context(patch.object(publisher.comments, "api"))
                output = stack.enter_context(contextlib.redirect_stdout(io.StringIO()))
                publisher.publish(100, 2, "ty-ecosystem", 42, dry_run=True)
                download.assert_called_once_with(self.artifact)
                self.assertEqual(find.call_args_list[0].args, (100, "comment.md"))
                self.assertEqual(find.call_args_list[1].args, (100, "full-report"))
                api.assert_not_called()
                self.assertIn("summary", output.getvalue())
                self.assertEqual(
                    "/runs/100/artifacts/300" in output.getvalue(),
                    full_report is not None,
                )


def suite() -> unittest.TestSuite:
    return unittest.TestSuite(
        PublisherScenarios(name)
        for name in (
            "reads_only_fixed_member_without_extracting",
            "missing_member_is_rejected",
            "duplicate_member_is_rejected",
            "uncompressed_size_is_bounded",
            "large_archive_is_rejected_before_download",
            "selects_latest_unexpired_artifact",
            "failed_source_can_publish",
            "new_attempt_invalidates_old_completion",
            "can_publish_while_other_ci_jobs_run",
            "rejects_fork_source",
            "rejects_wrong_producer",
            "rejects_stale_pr_head",
            "render_keeps_metadata_and_truncation_link",
            "returns_creation_response_id",
            "updates_stored_comment",
            "unchanged_report_returns_stored_id",
            "newer_published_run_wins",
            "changed_head_after_download_prevents_writes",
            "changed_pr_after_download_prevents_writes",
            "dry_run_does_not_write",
            "empty_report_does_not_write",
            "workflow_output_is_only_written_for_a_comment",
            "malformed_stored_id_prevents_publication",
            "truncated_summary_keeps_full_html_link",
            "ty_links_full_report_without_downloading_it",
        )
    )


if __name__ == "__main__":
    unittest.main(defaultTest="suite")
