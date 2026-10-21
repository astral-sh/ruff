# Pull request automation reports

`pull-request-comment.yml` attempts to reserve one of the first three issue comments when CI is
requested for a pull request, including a fork pull request. The workflow runs code from the trusted
default branch and uses `GITHUB_TOKEN` to post as `github-actions[bot]`. The comment is an index of
links to separate report comments.

The helper checks the bot's numeric identity, the owning pull request, and the comment marker before
using a comment. It finds the index with one request for the first three comments:

```sh
uv run --script scripts/pull_request_comments.py find --pull-request "$PULL_REQUEST"
```

Publishers look up their report through the index, then fetch that comment directly to verify its
identity and marker. A missing or deleted entry falls back to paginated discovery. This also supports
older pull requests whose first three comment positions are occupied.

```sh
uv run --script scripts/pull_request_comments.py lookup \
  --pull-request "$PULL_REQUEST" --report ruff-ecosystem
uv run --script scripts/pull_request_comments.py register \
  --pull-request "$PULL_REQUEST" --report ruff-ecosystem --comment-id "$COMMENT_ID"
```

Register the ID returned by GitHub after creating or updating the report. Registration verifies the
report and updates only its index entry. It leaves publication intact when there is no index. If a
human comment races the reservation into a later position, registration can still find that index.

All publishers must hold the job concurrency group
`pull-request-automation-comment-${{ needs.resolve.outputs.pull-request }}` with `queue: max` and
`cancel-in-progress: false` while looking up, publishing, and registering a report. Resolve the pull
request number before acquiring the group. This serializes report creation and index edits across
different workflows without discarding pending reports.

The available report keys are `ruff-ecosystem`, `memory`, `typing-conformance`, and `ty-ecosystem`.
Report bodies start with the corresponding marker in `REPORTS`, followed by a newline. Preserve the
index marker and its report-ID markers when updating the index.
