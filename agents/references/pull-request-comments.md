# Pull request automation reports

CI calls `pull-request-comment.yml` to attempt to reserve one of the first three issue comments for a pull request
whose source branch is in this repository. The reusable workflow runs code from its pinned revision
and uses STS to post as `astral-automations-bot[bot]`. The comment is an index of links to separate
report comments.

The caller passes `STS_API_URL` by name so the reusable job can use its value from the `automations`
environment. The repository's STS policy authorizes the CI caller on `refs/pull/*/merge` and the exact
reusable workflow revision for `pull_requests: write`. Fork pull requests are excluded. Changes to
the reusable workflow or its script require updating both its caller and the policy pin.

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
`pull-request-automation-comment-${{ github.event.pull_request.number }}` with `queue: max` and
`cancel-in-progress: false` while looking up, publishing, and registering a report. This serializes
report creation and index edits across different workflows and allows multiple jobs to wait.

The available report keys are `ruff-ecosystem`, `memory`, `typing-conformance`, and `ty-ecosystem`.
Report bodies start with the corresponding marker in `REPORTS`, followed by a newline. Preserve the
index marker and its report-ID markers when updating the index.
