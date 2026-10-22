# /// script
# requires-python = ">=3.13"
# dependencies = []
# ///
"""Exercise reviewer rotation, state migration, and GitHub request boundaries."""

from __future__ import annotations

import contextlib
import io
import json
import random
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch
from urllib.error import HTTPError

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import pull_request_reviewers as reviewers


class FixedRandom(random.Random):
    def __init__(self, value=0.0):
        super().__init__(0)
        self.value = value

    def random(self):
        return self.value

    def choice(self, sequence):
        return sequence[0]


def pull(*, author="author", number=42, **overrides):
    return {
        "number": number,
        "state": "open",
        "draft": False,
        "user": {"login": author},
        "requested_reviewers": [],
        "requested_teams": [],
        "base": {"repo": {"id": reviewers.REPOSITORY_ID}},
        "head": {"repo": {"id": reviewers.REPOSITORY_ID}, "sha": "a" * 40},
    } | overrides


def exercise(
    *, state=None, pools=None, responses=None, dry_run=False, number=42, author="author"
):
    state = {} if state is None else state
    pools = pools or [reviewers.Pool("core", ("src/**",), ("alice", "bob"))]
    if responses is None:
        responses = [
            pull(number=number, author=author),
            [{"filename": "src/file.py"}],
            pull(number=number, author=author),
            {},
        ]
    github = Mock(side_effect=responses)
    with (
        patch.object(reviewers, "api", github),
        contextlib.redirect_stdout(io.StringIO()),
    ):
        result = reviewers.assign_reviewer(
            number, pools, state, dry_run=dry_run, rng=FixedRandom()
        )
    return result, state, github


class ReviewerScenarios(unittest.TestCase):
    def normalizes_config_and_deduplicates_case_insensitively(self):
        pools = reviewers.parse_pools("""
[[pools]]
name = " Core "
paths = ["/src/**", "/src/**", "tests/?est_*.py"]
reviewers = ["@Alice", "bob", "@alice", " "]
[[pools]]
name = "core"
paths = ["docs/**"]
reviewers = ["docs"]
""")
        self.assertEqual(
            pools,
            [reviewers.Pool("Core", ("src/**", "tests/?est_*.py"), ("Alice", "bob"))],
        )

    def modern_config_takes_precedence_over_legacy(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            legacy = directory / "pr-assignee-pools.toml"
            legacy.write_text(
                '[[pools]]\nname="legacy"\npaths=["**"]\nreviewers=["alice"]\n'
            )
            self.assertEqual(reviewers.load_pools(directory)[0].name, "legacy")
            (directory / "pr-reviewer-pools.toml").write_text("pools=[]\n")
            self.assertEqual(reviewers.load_pools(directory), [])

    def overlapping_patterns_count_each_changed_file_once(self):
        first = reviewers.Pool("first", ("src/**", "src/lib/**"), ("alice",))
        second = reviewers.Pool("second", ("src/lib/**",), ("bob",))
        matched = reviewers.matching_pools(
            [first, second], ["src/lib/a.py", "src/other.py", "docs/a.md"]
        )
        self.assertEqual(matched, [(first, 2), (second, 1)])
        self.assertEqual(reviewers.choose_pool(matched, FixedRandom(0.5)), first)
        self.assertEqual(reviewers.choose_pool(matched, FixedRandom(0.9)), second)

    def changed_files_are_paginated(self):
        batch = [{"filename": f"src/{i}.py"} for i in range(100)]
        with patch.object(
            reviewers, "api", side_effect=[batch, [{"filename": "last.py"}]]
        ) as github:
            self.assertEqual(len(reviewers.changed_files(42)), 101)
        self.assertEqual(github.call_args.args[0], "pulls/42/files?per_page=100&page=2")

    def credits_rotate_across_pull_requests(self):
        pool = reviewers.Pool("core", ("src/**",), ("alice", "bob", "carl"))
        state = {}
        selected = []
        for number in (42, 43, 44):
            result, _, github = exercise(state=state, pools=[pool], number=number)
            selected.append(result[1])
            self.assertEqual(
                github.call_args.kwargs["data"], {"reviewers": [result[1]]}
            )
        self.assertEqual(selected, ["alice", "bob", "carl"])
        for credit in state["core"].values():
            self.assertAlmostEqual(credit, 0)

    def author_earns_credit_but_cannot_be_selected(self):
        pool = reviewers.Pool("core", ("src/**",), ("Alice", "bob"))
        result, state, _ = exercise(
            pools=[pool],
            state={"core": {"Alice": 0.9, "bob": 0}},
            author="aLiCe",
        )
        self.assertEqual(result, ("core", "bob"))
        self.assertEqual(state, {"core": {"Alice": 1, "bob": -0.5}})

    def highest_credit_wins_and_other_pool_state_survives(self):
        result, state, _ = exercise(
            state={
                "core": {"alice": 0, "bob": 0.9, "removed": 1},
                "docs": {"eve": -0.2},
            }
        )
        self.assertEqual(result, ("core", "bob"))
        self.assertEqual(
            state, {"core": {"alice": 0.5, "bob": 0}, "docs": {"eve": -0.2}}
        )

    def legacy_integer_counters_seed_zero_credits(self):
        result, state, _ = exercise(state=reviewers.parse_state('{"core": 7}'))
        self.assertEqual(result, ("core", "alice"))
        self.assertEqual(state["core"], {"alice": -0.5, "bob": 0.5})

    def ineligible_pull_requests_do_not_list_files_or_write(self):
        invalid = [pull(state="closed"), pull(draft=True), pull(number=43)]
        fork = pull()
        fork["head"]["repo"]["id"] = 1
        invalid.append(fork)
        deleted = pull()
        deleted["head"]["repo"] = None
        invalid.append(deleted)
        for current in invalid:
            with self.subTest(current=current):
                result, state, github = exercise(responses=[current])
                self.assertIsNone(result)
                self.assertEqual(state, {})
                github.assert_called_once_with("pulls/42")

    def existing_direct_reviewer_prevents_assignment(self):
        result, state, github = exercise(
            responses=[
                pull(requested_reviewers=[{"login": "author"}, {"login": "manual"}])
            ]
        )
        self.assertIsNone(result)
        self.assertEqual(state, {})
        github.assert_called_once_with("pulls/42")

    def team_requests_and_author_do_not_block_assignment(self):
        current = pull(
            requested_reviewers=[{"login": "AUTHOR"}],
            requested_teams=[{"slug": "team"}],
        )
        result, _, github = exercise(
            responses=[current, [{"filename": "src/a.py"}], current, {}]
        )
        self.assertEqual(result, ("core", "alice"))
        self.assertEqual(github.call_args.kwargs["data"], {"reviewers": ["alice"]})

    def newly_requested_reviewer_prevents_assignment(self):
        result, state, github = exercise(
            responses=[
                pull(),
                [{"filename": "src/a.py"}],
                pull(requested_reviewers=[{"login": "manual"}]),
            ]
        )
        self.assertIsNone(result)
        self.assertEqual(state, {})
        self.assertTrue(
            all("data" not in call.kwargs for call in github.call_args_list)
        )

    def changed_head_prevents_assignment(self):
        changed = pull()
        changed["head"]["sha"] = "b" * 40
        result, state, github = exercise(
            responses=[pull(), [{"filename": "src/a.py"}], changed]
        )
        self.assertIsNone(result)
        self.assertEqual(state, {})
        self.assertTrue(
            all("data" not in call.kwargs for call in github.call_args_list)
        )

    def rejected_candidate_tries_another_reviewer(self):
        error = HTTPError("https://api.github.com/", 422, "Rejected", {}, None)
        result, state, github = exercise(
            responses=[
                pull(),
                [{"filename": "src/a.py"}],
                pull(),
                error,
                pull(),
                {},
            ]
        )
        self.assertEqual(result, ("core", "bob"))
        self.assertEqual(state["core"], {"alice": 0.5, "bob": -0.5})
        self.assertEqual(
            [
                call.kwargs["data"]
                for call in github.call_args_list
                if "data" in call.kwargs
            ],
            [{"reviewers": ["alice"]}, {"reviewers": ["bob"]}],
        )

    def exhausted_pool_tries_another_matching_pool(self):
        pools = [
            reviewers.Pool("first", ("src/**",), ("alice",)),
            reviewers.Pool("second", ("src/**",), ("bob",)),
        ]
        result, state, _ = exercise(
            pools=pools,
            responses=[
                pull(),
                [{"filename": "src/a.py"}],
                pull(),
                HTTPError("https://api.github.com/", 422, "Rejected", {}, None),
                pull(),
                {},
            ],
        )
        self.assertEqual(result, ("second", "bob"))
        self.assertEqual(state, {"second": {"bob": 0}})

    def author_only_pool_does_not_change_credits(self):
        pool = reviewers.Pool("core", ("src/**",), ("author",))
        result, state, github = exercise(pools=[pool], state={"core": {"author": 0.5}})
        self.assertIsNone(result)
        self.assertEqual(state, {"core": {"author": 0.5}})
        self.assertTrue(
            all("data" not in call.kwargs for call in github.call_args_list)
        )

    def unexpected_github_error_does_not_advance_credits(self):
        state = {}
        with self.assertRaises(HTTPError):
            exercise(
                state=state,
                responses=[
                    pull(),
                    [{"filename": "src/a.py"}],
                    pull(),
                    HTTPError("https://api.github.com/", 500, "Unavailable", {}, None),
                ],
            )
        self.assertEqual(state, {})

    def dry_run_does_not_request_review_or_change_state(self):
        result, state, github = exercise(dry_run=True)
        self.assertEqual(result, ("core", "alice"))
        self.assertEqual(state, {})
        self.assertTrue(
            all("data" not in call.kwargs for call in github.call_args_list)
        )

    def stored_state_takes_precedence_over_bootstrap_variable(self):
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory) / "state.json"
            self.assertEqual(
                reviewers.load_state(state, '{"core":{"alice":0.5}}'),
                {"core": {"alice": 0.5}},
            )
            state.write_text('{"core":{"alice":-0.5}}')
            self.assertEqual(
                reviewers.load_state(state, "invalid"), {"core": {"alice": -0.5}}
            )
            self.assertEqual(reviewers.load_state(Path(directory) / "missing", ""), {})

    def invalid_state_fails_instead_of_resetting_fairness(self):
        for contents in (
            "invalid",
            "[]",
            '{"core": -1}',
            '{"core": true}',
            '{"core": {"alice": NaN}}',
            '{"core": {"alice": Infinity}}',
            '{"core": {"alice": "0"}}',
        ):
            with (
                self.subTest(contents=contents),
                self.assertRaises((ValueError, TypeError)),
            ):
                reviewers.parse_state(contents)

    def invalid_state_prevents_any_github_request(self):
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory) / "state"
            state.write_text("invalid")
            argv = [
                "reviewers",
                "--pull-request",
                "42",
                "--state",
                str(state),
                "--output",
                str(Path(directory) / "next"),
            ]
            with (
                patch.object(sys, "argv", argv),
                patch.object(reviewers, "api") as github,
                self.assertRaises(ValueError),
            ):
                reviewers.main()
            github.assert_not_called()

    def noop_writes_state_for_retention_and_dry_run_does_not(self):
        for dry_run in (False, True):
            with (
                self.subTest(dry_run=dry_run),
                tempfile.TemporaryDirectory() as directory,
            ):
                state = Path(directory) / "state"
                output = Path(directory) / "next"
                state.write_text('{"core":{"alice":0.5}}')
                argv = [
                    "reviewers",
                    "--pull-request",
                    "42",
                    "--state",
                    str(state),
                    "--output",
                    str(output),
                ]
                if dry_run:
                    argv.append("--dry-run")
                with (
                    patch.object(sys, "argv", argv),
                    patch.object(
                        reviewers,
                        "api",
                        return_value=pull(requested_reviewers=[{"login": "manual"}]),
                    ),
                ):
                    reviewers.main()
                if dry_run:
                    self.assertFalse(output.exists())
                else:
                    self.assertEqual(
                        json.loads(output.read_text()), {"core": {"alice": 0.5}}
                    )


def suite() -> unittest.TestSuite:
    return unittest.TestSuite(
        ReviewerScenarios(name)
        for name in (
            "normalizes_config_and_deduplicates_case_insensitively",
            "modern_config_takes_precedence_over_legacy",
            "overlapping_patterns_count_each_changed_file_once",
            "changed_files_are_paginated",
            "credits_rotate_across_pull_requests",
            "author_earns_credit_but_cannot_be_selected",
            "highest_credit_wins_and_other_pool_state_survives",
            "legacy_integer_counters_seed_zero_credits",
            "ineligible_pull_requests_do_not_list_files_or_write",
            "existing_direct_reviewer_prevents_assignment",
            "team_requests_and_author_do_not_block_assignment",
            "newly_requested_reviewer_prevents_assignment",
            "changed_head_prevents_assignment",
            "rejected_candidate_tries_another_reviewer",
            "exhausted_pool_tries_another_matching_pool",
            "author_only_pool_does_not_change_credits",
            "unexpected_github_error_does_not_advance_credits",
            "dry_run_does_not_request_review_or_change_state",
            "stored_state_takes_precedence_over_bootstrap_variable",
            "invalid_state_fails_instead_of_resetting_fairness",
            "invalid_state_prevents_any_github_request",
            "noop_writes_state_for_retention_and_dry_run_does_not",
        )
    )


if __name__ == "__main__":
    unittest.main(defaultTest="suite")
