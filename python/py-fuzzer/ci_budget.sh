#!/usr/bin/env bash

# Calculate the budget for `fuzz --budget-minutes` from a CI job's timeout and
# the time spent on setup.
#
# Pass the job timeout in minutes as the first argument, and set the
# FUZZ_JOB_STARTED_AT environment variable to the Unix timestamp in seconds
# when the job's first step starts (e.g., with `date +%s`). The script prints
# the resulting budget in minutes to stdout.

set -euo pipefail

job_timeout_minutes=$1
elapsed_seconds=$(( $(date +%s) - FUZZ_JOB_STARTED_AT ))

# Leave a minute for job startup before the first step and for post-fuzzing steps.
budget_minutes=$(( (job_timeout_minutes * 60 - elapsed_seconds - 60) / 60 ))

echo "Setup took ${elapsed_seconds}s; the fuzzer budget is ${budget_minutes} minutes." >&2
echo "$budget_minutes"
