# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Exercise report lookup with untrusted and stale cached comment IDs."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import pull_request_comments as comments


def report_comment(comment_id: int = 23) -> dict:
    return {
        "id": comment_id,
        "user": {"id": comments.BOT_ID, "type": "Bot"},
        "issue_url": f"{comments.API_ROOT}/issues/42",
        "body": f"{comments.REPORTS['memory'].marker}\nreport",
    }


def cached_id_avoids_discovery() -> None:
    expected = report_comment()
    with patch.object(comments, "api", return_value=expected) as api:
        assert comments.find_report(42, "memory", 23) == expected
    api.assert_called_once_with("issues/comments/23", missing_ok=True)


def deleted_comment_falls_back() -> None:
    expected = report_comment(24)
    with patch.object(comments, "api", side_effect=[None, [expected]]) as api:
        assert comments.find_report(42, "memory", 23) == expected
    assert api.call_args.args[0] == "issues/42/comments?per_page=100&page=1"


def foreign_comment_falls_back() -> None:
    expected = report_comment(24)
    invalid = [
        {"user": {"id": 1, "type": "Bot"}},
        {"user": {"id": comments.BOT_ID, "type": "User"}},
        {"issue_url": f"{comments.API_ROOT}/issues/43"},
        {"body": f"{comments.REPORTS['ruff-ecosystem'].marker}\nreport"},
        {"body": None},
    ]
    for override in invalid:
        cached = report_comment() | override
        with patch.object(comments, "api", side_effect=[cached, [expected]]):
            assert comments.find_report(42, "memory", 23) == expected


def missing_id_discovers_later_pages() -> None:
    expected = report_comment()
    unrelated = {"user": None, "body": None}
    with patch.object(
        comments, "api", side_effect=[[unrelated] * 100, [expected]]
    ) as api:
        assert comments.find_report(42, "memory") == expected
    assert api.call_count == 2
    assert api.call_args.args[0].endswith("page=2")


def no_report_returns_none() -> None:
    with patch.object(comments, "api", return_value=[]):
        assert comments.find_report(42, "memory") is None


def malformed_ids_are_cache_misses() -> None:
    for value in ("", "0", "-1", "1.0", "\uff11\uff12", "1\ncommand", "9" * 21):
        assert comments.parse_comment_id(value) is None
    assert comments.parse_comment_id(" 23\n") == 23


def api_failures_are_not_missing_comments() -> None:
    with patch.object(comments, "api", side_effect=RuntimeError("unavailable")):
        try:
            comments.find_report(42, "memory", 23)
        except RuntimeError:
            pass
        else:
            raise AssertionError("GitHub API failures must propagate")


def suite() -> unittest.TestSuite:
    return unittest.TestSuite(
        unittest.FunctionTestCase(case)
        for case in (
            cached_id_avoids_discovery,
            deleted_comment_falls_back,
            foreign_comment_falls_back,
            missing_id_discovers_later_pages,
            no_report_returns_none,
            malformed_ids_are_cache_misses,
            api_failures_are_not_missing_comments,
        )
    )


if __name__ == "__main__":
    unittest.main(defaultTest="suite")
