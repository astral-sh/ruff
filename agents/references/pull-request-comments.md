# Pull request report comments

Use `scripts/pull_request_comments.py` to find an automation report by its cached comment ID.
The helper verifies the bot's numeric identity, the owning pull request, and the report marker
before returning a comment. Missing, deleted, malformed, or mismatched IDs fall back to paginated
comment discovery.

```sh
uv run --script scripts/pull_request_comments.py \
  --pull-request "$PULL_REQUEST" --report ruff-ecosystem --comment-id "$CACHED_COMMENT_ID"
```

Set `GH_TOKEN` to a GitHub token with access to the PR. Publishing uses an STS token for
`astral-automations-bot[bot]`; comment lookup only accepts reports authored by that bot.

## Key-value storage

Store each report's comment ID in a separate key named `report-comment-<report>`. The available
report keys are `ruff-ecosystem`, `memory`, `typing-conformance`, and `ty-ecosystem`. Values contain
only a positive decimal comment ID. Write the ID returned by GitHub after creating or updating the
comment, including when discovery recovers an existing comment.

The key-value service derives its namespace from GitHub's signed OIDC context. Publishers for the
same PR must use the same event, ref, and `automations` environment to share state. Different PRs
receive separate namespaces. Other jobs in the same context can replace cached values, so an ID
must pass the helper's author, PR, and marker checks before use.

Entries expire according to the service's retention policy. Discovery recovers from expiration and
from a successful comment creation followed by a failed cache write. Cache availability must not
prevent a report from being published.

Serialize each report's lookup, publication, and cache write with the concurrency group
`pull-request-report-${{ github.event.pull_request.number }}-${{ inputs.report }}`, using
`queue: max` and `cancel-in-progress: false`.
