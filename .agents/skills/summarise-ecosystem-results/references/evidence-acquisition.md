# Freeze Ecosystem Evidence

At the beginning of the summary request, preserve any detailed report or ecosystem-results comment explicitly supplied by the user. Only look up the PR's current comment when the user provided no specific report or comment. Save the selected deployed report immediately when it is accessible, then identify its matching PR, Actions run, and attempt; never substitute a newer comment, report, PR revision, workflow run, or reporting attempt.

If no comment matching an explicitly supplied report remains, continue with the supplied report and record that the matching comment is unavailable. If the exact Actions run or attempt cannot be identified uniquely, report that uncertainty instead of selecting the current PR report or guessing from matching Ruff revisions.

Create a unique snapshot directory, save the matching comment when available and the selected attempt's effective job graph, and inspect the run's available artifacts:

```bash
snapshot_dir="$(mktemp -d "${TMPDIR:-/tmp}/ty-ecosystem-report.XXXXXX")"
ecosystem_comment_id="<matching-comment-id-or-empty>"
if [[ -n "$ecosystem_comment_id" ]]; then
  gh api "repos/astral-sh/ruff/issues/comments/$ecosystem_comment_id" > "$snapshot_dir/comment.json"
fi
gh run view <actions-run> --repo astral-sh/ruff --attempt <actions-attempt> \
  --json attempt,headSha,jobs,startedAt,updatedAt,url > "$snapshot_dir/run.json"
gh api "repos/astral-sh/ruff/actions/runs/<actions-run>/artifacts" > "$snapshot_dir/artifacts.json"
```

`gh run download` cannot select an attempt, and a newer rerun can replace an older attempt's artifacts without changing Ruff's revisions. Before downloading, verify that `full-report` was created during the selected attempt's report-generation job and that each diagnostics shard was created during its matching successful shard job. Use the effective job graph, not the attempt start time: partial reruns legitimately inherit successful jobs and artifacts from earlier attempts.

Download only artifacts that pass these checks; use the shard glob only when every matching artifact belongs to the selected job graph:

```bash
gh run download <actions-run> --repo astral-sh/ruff --name full-report --dir "$snapshot_dir/full-report"
gh run download <actions-run> --repo astral-sh/ruff --pattern 'diagnostics-shard-*' --dir "$snapshot_dir/shards"
```

Record the selected Actions attempt and pass it to `scripts/collect_ty_ecosystem_run_metadata.py` with `--attempt <actions-attempt>`. Verify that the frozen report's Ruff base and PR revisions agree with the resulting manifest, then use the saved report, shards, run, attempt, and matching comment when available throughout the investigation.

If the selected report's artifacts were replaced or are unavailable, use its frozen deployed report and explicitly describe any unavailable shards or resulting verification limitations. Never silently substitute artifacts produced by an unrelated reporting attempt.
