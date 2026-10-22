# Pull request reviewer selection

`pull-request-reviewers.yml` attempts to select a reviewer when a same-repository pull request is opened,
reopened, or marked ready for review. Draft and closed PRs are excluded. The selection uses
`.github/pr-reviewer-pools.toml` from `main`, with `.github/pr-assignee-pools.toml` as a fallback when
the current configuration file is absent.

## Workflow and credentials

The actions dispatch service reads `.github/automations-dispatch.json` from Ruff's `main` branch,
verifies signed `pull_request` webhooks, and dispatches the worker on `main` with the canonical
`pull_request` and `head_sha` inputs. The worker checks out its own commit on `main`, reads the PR
and changed-file metadata through GitHub's API, and uses an STS token with
`pull_requests: write` to request a review as `astral-automations-bot[bot]`. It skips requests whose
head commit is no longer current and does not execute PR code.

The worker uses the `automations` environment. The repository STS policy authorizes it on
`refs/heads/main` for `workflow_dispatch`.

Set the repository variable `KV_URL` to the deployed key-value service's HTTPS origin and provide
`STS_API_URL` in the `automations` environment. The key-value service must allow Ruff's repository
identity. The dispatcher must support repository-owned policies, use
`.github/automations-dispatch.json` as its policy path, and receive Ruff's `pull_request` webhooks.
Keep only one reviewer assigner active when enabling this workflow.

## Shared state

The worker uses the `key-value-store/get` and `key-value-store/put` actions from
`astral-sh/github-actions` for the `reviewer-round-robin-v1` key. Its namespace uses the
`workflow_dispatch` event, `refs/heads/main`, and the `automations` environment, so reviewer credits
are shared across PRs. PR-scoped report comment IDs live in separate namespaces.

A global `pull-request-reviewer-round-robin` concurrency group with `queue: max` and
`cancel-in-progress: false` serializes the read, reviewer request, and write. The key-value API does
not need compare-and-swap or a new namespace type. Other workflows using this namespace must not
write the reviewer key outside this concurrency group.

When the key is absent or expired, the worker seeds credits from `PR_REVIEWER_ROUND_ROBIN_STATE`,
falling back to `PR_ASSIGNEE_ROUND_ROBIN_STATE`. Missing variables start with zero credits. Integer
counters from the older state format also start with zero credits. The worker does not write either
repository variable. KV read failures and malformed state stop the worker before any review request.

Every successful worker writes its state, including when no reviewer is requested, to renew the
service's retention period. GitHub review requests and KV writes are not atomic: if the write fails
after a review request succeeds, the request remains and the stored credits may be stale. A rerun
respects that existing review request while it is still pending.

## Selection

Each pool is weighted by the number of changed files matching its patterns. The worker chooses a
pool by those weights, then adds `1 / member_count` to each member's credit, capped at `1`.
The PR author earns credit but cannot be selected. Among eligible reviewers, the highest
credit wins, with random tie-breaking. A successful request saves these credits and subtracts `1`
from that reviewer's credit, with a minimum of `-1`.

Existing direct review requests prevent automatic selection; team requests do not. The worker
rechecks the PR before each request. A `422` response tries another candidate, then another matching
pool if needed. Failed requests do not advance credits.

To preview a selection from a trusted checkout without requesting a review or writing state, set
`GH_TOKEN` to a GitHub token, `PULL_REQUEST` to the PR number, `HEAD_SHA` to its current head commit,
and `REVIEWER_STATE` to a JSON state file path. An absent state file uses the legacy state from
`LEGACY_REVIEWER_STATE`, or zero credits when that variable is unset.

```sh
uv run --script scripts/pull_request_reviewers.py \
  --pull-request "$PULL_REQUEST" --head-sha "$HEAD_SHA" --state "$REVIEWER_STATE" --dry-run
```
