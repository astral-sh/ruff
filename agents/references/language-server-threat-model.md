# Ruff and ty language server threat model

## Overview

The [CLI threat model](cli-threat-model.md) applies to the language servers. The workspace trust rules
below take precedence when deciding which inputs are trusted. Browser integrations have a separate
[playground model](playground-threat-model.md).

## Trust boundaries and assumptions

- **Attacker-controlled:** all inputs analyzed in an untrusted workspace, including source code,
    project configuration, and notebooks.
- **Trusted local input:** the editor, its extensions, the local machine, including its file system,
    and the user's configuration and trust decisions.

ty treats workspaces as trusted unless `untrustedWorkspace` is true at initialization. Trust extends
to everything in the workspace, including source code, configuration, notebooks, and the targets of
symlinks that point outside it.

## Security invariants

- **Code Execution:** When `untrustedWorkspace` is true, ty must not execute code related to the
    workspace or its dependencies, including code run during installation.
- **Edits:** Editing operations must not change unrelated files.
- **Output:** Diagnostics, documentation, and logs must not allow attacker-controlled text to run
    code or invoke editor commands.
- **Availability:** An isolated parser panic or slow analysis is a correctness or performance bug.
    A security issue requires repeatable, disproportionate resource use that materially disrupts
    the editor or the developer's machine beyond the failed analysis.

## Severity calibration

- **Critical:** With few prerequisites and safe defaults, an attacker gains broad execution
    or credential access.
- **High:** A demonstrated boundary crossing causes substantial confidentiality or integrity harm,
    such as executing attacker code despite an effective untrusted-workspace restriction.
- **Medium:** A limited unauthorized read or write, or bypass of an execution restriction with
    limited effect.
- **Low:** A narrow disclosure or safety gap with limited practical impact, or reliable resource
    exhaustion affecting the editor or the developer's machine.
- **Informational:** A genuine security concern with negligible current impact.
