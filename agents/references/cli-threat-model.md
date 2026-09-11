# Ruff and ty CLI threat model

## Overview

Ruff checks and formats Python code. ty checks Python types. Their commands and native libraries read
source files, configuration, and dependencies; produce diagnostics; and edit files when requested.
They run with the user's or CI worker's permissions.

A behavior is a security issue only when an independent attacker controls a concrete input, Ruff or
ty uses it to cross a boundary defined below, and the crossing gives the attacker new power or harms
a protected asset. Trusted-machine compromise, intended behavior, and correctness defects that give
an attacker no new power are not security issues.

Editors, playgrounds, and repository automation have [separate models](threat-models.md).

## Trust boundaries and assumptions

- **Attacker-controlled:** source code, stubs, notebook contents, Markdown, and project configuration.
- **Trusted local input:** the operating system, installed programs, environment variables, `PATH`,
    caches, user configuration, explicit command-line choices, and user-managed filesystem state that
    the attacker cannot change.

## Security invariants

- **Code Execution:** Analysis must not execute user-supplied code.
- **Configuration and file discovery:** Ruff's `--isolated` must ignore configuration files. Imports,
    configuration extensions, and symlinks may lead outside the project; reading those files is
    expected.
- **Edits and cleanup:** Formatting, fixes, ignore insertion, and cache cleanup may change only the
    targets selected by that operation. Formatting with `--check` must preserve source files.
    Incorrect fixes are ordinarily correctness bugs.
- **External programs:** ty's uv integration is disabled by default. When enabled, it uses the
    executable selected by `UV` or `PATH` for workspace metadata and script environments. Script
    synchronization may install dependencies.
- **Output:** Diagnostics may contain source text and paths. They must be encoded for the selected
    terminal or CI format so that attacker-controlled text cannot inject commands or active content.
- **Availability:** An isolated parser panic or slow analysis is a correctness or performance bug.
    A security issue requires repeatable, disproportionate resource use that materially disrupts
    the developer's machine or CI worker beyond the failed analysis.

## Severity calibration

- **Critical:** With few prerequisites and safe defaults, analysis input compromises credentials
    with broad permissions or causes widespread file damage without first compromising a trusted host.
- **High:** A demonstrated path from analysis input to arbitrary native execution, substantial
    disclosure of private data, or destructive filesystem access beyond the requested operation.
- **Medium:** A limited unauthorized read or write, or bypass of an execution restriction with
    limited effect.
- **Low:** A narrow disclosure or safety gap across a real boundary with limited practical impact,
    or reliable resource exhaustion affecting the developer's machine or CI worker.
- **Informational:** A genuine security concern with negligible current impact.
