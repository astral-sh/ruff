# Pull request report comments

`publish-pull-request-report.yml` provides a shared publisher for same-repository pull requests.
Callers pass a `report` input and the `KV_API_URL` and `STS_API_URL` secrets, and pin the reusable
workflow to the same revision authorized by the STS policy. The publisher runs code from that revision in the
`automations` environment. STS supplies the `actions: read` and `pull_requests: write` token used to
publish as `astral-automations-bot[bot]`.

## Comment IDs

The publisher uses `key-value-store/get` and `key-value-store/put` from `astral-sh/github-actions`.
Set the `KV_API_URL` secret in the `automations` environment to the service's HTTPS URL. The actions
exchange the job's OIDC token for access to the service independently of STS.

The key-value store is the only source of existing comment IDs. The publisher fetches a stored ID
directly and verifies the bot's numeric identity, the owning PR, and the marker derived from the
`report` input. It never lists or scans PR comments. A missing or expired key, or a deleted comment,
causes a new comment to be created and its ID stored. Malformed IDs and mismatched comments fail
publication.

Each report uses a separate key named `report-comment-<report>`, containing a positive decimal ID.
The report names are `ruff-ecosystem`, `memory`, `typing-conformance`, and `ty-ecosystem`. GitHub's
signed OIDC context scopes these keys to the PR event, merge ref, and `automations` environment.
Other jobs in the same context can write these keys, so stored IDs must pass the identity checks.

A failed KV read stops publication. A failed write fails the job after the comment has been
published. If the publisher created or updated a comment, its ID is available in that step's log for
recovery. Rerunning without restoring the ID can create another comment. Successful writes renew the
service's retention period, including when the comment is unchanged or already contains a newer report.

The job holds the concurrency group
`pull-request-report-${{ github.event.pull_request.number }}-${{ inputs.report }}` with `queue: max`
and `cancel-in-progress: false` through lookup, publication, and storage.

## Report artifacts

The publisher checks the source workflow, repository, current PR head, and run attempt through
GitHub's API. It reads one fixed ZIP member in memory without extracting or executing the artifact.
Reports can be published when the producer fails after uploading its artifact. Missing or empty
artifacts do not produce comments. Each comment records the run and attempt, so an older publication
cannot overwrite a newer report. A partial rerun can reuse an artifact from an earlier attempt of
the same run. The publisher rechecks the PR head before writing. ty ecosystem summaries link to the
full HTML reports artifact when it is available.

With `gh` installed and `GH_TOKEN` set to a token with read access, render a report without posting it:

```sh
uv run --script scripts/publish_pr_report.py \
  --run-id "$RUN_ID" --run-attempt "$RUN_ATTEMPT" \
  --report ruff-ecosystem --pull-request "$PULL_REQUEST" --dry-run
```
