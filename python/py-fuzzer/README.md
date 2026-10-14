# py-fuzzer

A fuzzer script to run Ruff executables on randomly generated
(but syntactically valid) Python source-code files.

Run `uv run --project=./python/py-fuzzer fuzz -h` from the repository root
for more information and example invocations
(requires [`uv`](https://github.com/astral-sh/uv) to be installed).

The `--write-github-issue` option is intended for CI workflows. It writes a Markdown issue body when the fuzzer finds bugs. The report includes as many reproducers as fit in a GitHub issue, with the original generated source when minimization fails or times out. When writing a report, the fuzzer requires `GITHUB_SERVER_URL`, `GITHUB_REPOSITORY`, and `GITHUB_RUN_ID` to be set. If any are missing, it exits with an error. These are the GitHub server URL, `OWNER/REPO`, and run ID from the workflow URL; GitHub Actions sets them automatically.

For example, a GitHub Actions workflow can build Ruff and fuzz it using seeds 0 through 10 inclusive:

```shell
uv run --project=./python/py-fuzzer fuzz --bin=ruff --write-github-issue=fuzz-issue.md 0-10
```

The report identifies the test executable's build commit. The file is only written if bugs are found. The fuzzer exits with status 1 when it finds bugs and 0 when it finds none.
