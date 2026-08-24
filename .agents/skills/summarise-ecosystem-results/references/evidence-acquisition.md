# Freeze Ecosystem Evidence

At the beginning of the summary request, preserve any detailed report or ecosystem-results comment explicitly supplied by the user. Only look up the PR's current comment when the user provided no specific report or comment. Save the selected deployed report immediately when it is accessible, then identify its matching PR, Actions run, and attempt; never substitute a newer comment, report, PR revision, workflow run, or reporting attempt.

If no comment matching an explicitly supplied report remains, continue with the supplied report and record that the matching comment is unavailable. If the exact Actions run or attempt cannot be identified uniquely, report that uncertainty instead of selecting the current PR report or guessing from matching Ruff revisions.

Create a unique snapshot directory, save the matching comment when available and the selected attempt's effective job graph, and inspect the run's available artifacts:

```bash
set -euo pipefail

snapshot_dir="$(mktemp -d "${TMPDIR:-/tmp}/ty-ecosystem-report.XXXXXX")"
ecosystem_comment_id="<matching-comment-id-or-empty>"
if [[ -n "$ecosystem_comment_id" ]]; then
  GH_TELEMETRY=false gh api "repos/astral-sh/ruff/issues/comments/$ecosystem_comment_id" > "$snapshot_dir/comment.json"
fi
GH_TELEMETRY=false gh run view <actions-run> --repo astral-sh/ruff --attempt <actions-attempt> \
  --json attempt,headSha,jobs,startedAt,updatedAt,url > "$snapshot_dir/run.json"
GH_TELEMETRY=false gh api --paginate --slurp \
  "repos/astral-sh/ruff/actions/runs/<actions-run>/artifacts?per_page=100" |
  jq '{artifacts: [.[].artifacts[]]}' > "$snapshot_dir/artifacts.json"
printf 'TY_ECOSYSTEM_SNAPSHOT_DIR=%s\n' "$snapshot_dir"
```

`gh run download` cannot select an attempt, and a newer rerun can replace an older attempt's artifacts without changing Ruff's revisions. Before downloading, verify that `full-report` was created during the selected attempt's report-generation job and that each diagnostics shard was created during its matching successful shard job. Use the effective job graph, not the attempt start time: partial reruns legitimately inherit successful jobs and artifacts from earlier attempts. Preserve each validated artifact's immutable ID; never re-resolve its mutable name during download.

Download the validated report and each available, validated shard directly by artifact ID:

```bash
download_validated_artifact() {
  local artifact_id="$1"
  local destination="$2"

  mkdir -p "$destination"
  GH_TELEMETRY=false gh api "repos/astral-sh/ruff/actions/artifacts/$artifact_id/zip" \
    > "$snapshot_dir/artifact-$artifact_id.zip"
  unzip -q "$snapshot_dir/artifact-$artifact_id.zip" -d "$destination"
}

download_validated_artifact <validated-full-report-id> "$snapshot_dir/full-report"
download_validated_artifact <validated-shard-id> \
  "$snapshot_dir/shards/diagnostics-shard-<number>"
```

When the selected deployed HTML report was available, compare it byte-for-byte with the downloaded artifact's `diff.html` before trusting the adjacent JSON. Record the selected Actions attempt and pass it to `scripts/collect_ty_ecosystem_run_metadata.py` with `--attempt <actions-attempt>` once for all projects requiring reproduction. Verify that the frozen HTML report's Ruff base and PR revisions agree with the resulting immutable manifest, then use the saved report, shards, run, attempt, and matching comment when available throughout the investigation.

## Prefer the Exact Attempt's Structured Diff

When the validated `full-report` artifact contains `diff.json`, use `$snapshot_dir/full-report/diff.json` as the authoritative structured change inventory and the adjacent `diff.html` as its human-readable counterpart. The JSON contains no Ruff revisions, Actions run ID, or attempt number: its provenance comes from the verified artifact and its matching HTML report, not from its contents or a coincidentally matching PR revision.

The deployment also exposes a sibling `diff.json` next to the detailed HTML report. Use a deployed JSON file only when it was frozen from the same immutable deployment as the selected HTML report and that deployment can be tied to the selected Actions run and attempt. A current PR deployment, a later artifact, or matching Ruff commits alone cannot establish this provenance.

Inspect the structured report and the schema or diff-generation code at the exact ecosystem-analyzer revision; do not assume JSON field names or classifications remain stable across revisions. Record each affected project's strict or non-strict analysis mode and include both strict-analysis flags in the comparison method when applicable.

If the selected attempt's JSON report or artifacts are unavailable or replaced, or its artifact HTML differs from the frozen deployed report, use the frozen HTML report, disclose unavailable shards and resulting verification limitations, and never substitute another attempt's artifacts.
