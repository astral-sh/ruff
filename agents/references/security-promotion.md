# Promoting security fixes

This port follows uv's promotion workflow at
[`3db665232544`](https://github.com/astral-sh/uv/blob/3db665232544183edcb80dd7077b8051b365e236/.github/workflows/promote-pull-request.yml).
The `$/.github/workflows/update-pull-request-parent.yml` reference uses GitHub's
[self-repository syntax](https://docs.github.com/en/actions/how-tos/reuse-automations/reuse-workflows#calling-a-reusable-workflow).
The actionlint configuration suppresses the unsupported-syntax diagnostic for this
reference until actionlint supports it.

A repository writer can publish a pull request from `astral-sh/ruff-security` by
adding `bot:promote`. Draft pull requests are eligible. Marking a pull request ready
for review does not authorize publication.

The promotion workflow verifies the human label event, the writer's current
permission, and the approved commit before copying the branch to `astral-sh/ruff`.
It creates or reuses a matching public pull request, copies its title, body, and
labels other than automation and triage labels, assigns the promoter, and closes
the private pull request. It does not merge the public pull request.

An unpublished private parent must be promoted first. When a promoted parent has
merged and synced back, the parent-update workflow can rebase the child commits
without resolving conflicts. Rejected promotions retain their draft status,
assign the promoter, and remove `bot:promote`. Review the reported problem and add
the label again to retry. For a queued child, remove and re-add `bot:promote` after
its parent becomes available.

## Service configuration

The workflow files alone do not activate promotion. Before using the label:

1. Merge the workflows and policies into public Ruff and let `sync-ruff-security.yml`
    fast-forward private `main`. The token service reads the policy from each target's
    protected `main` branch.
1. Configure the `astral-automations-bot` installation identified by
    `.github/secure-token-service.json` to cover both repositories, with the contents,
    pull-request, and workflow write permissions used by promotion.
1. Create the `automations` environment in `ruff-security`, restrict deployments to
    `main`, and make the `STS_API_URL` Actions secret available to its jobs.
1. Configure the actions dispatcher to load Ruff's `.github/automations-dispatch.json`
    from public `main`, and arrange delivery of `ruff-security` pull-request events
    to that dispatcher. The dispatcher has a statically configured policy source;
    adding a policy file to a repository does not register it. If using a shared
    dispatcher, add the Ruff repository identity and rule to its configured policy
    instead of replacing the routes for other repositories.

Use the same GitHub App identity for promotion and recovery. The workflow checks
that identity when verifying earlier parent promotions and updating recovery
comments.

## Inherited parent-handling limitations

Before a public parent merge has synced to the private repository, the comparison
request can fail with HTTP 404 instead of posting the waiting comment. An open
public pull request from an unrelated fork with the same branch name can also
prevent discovery of the private or merged parent. Both behaviors are inherited
from the uv workflow.

## Local validation

Run `uv run scripts/check_security_promotion.py` to exercise approval checks,
rejection recovery, and private-to-public Git transfers using temporary local
repositories and a fake GitHub API. These scenarios do not verify deployed App
permissions, dispatcher routing, or token exchange.
