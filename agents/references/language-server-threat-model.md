# Ruff and ty language server threat model

## Overview

This model covers the Ruff and ty language servers and their interactions with editors. The
[CLI threat model](cli-threat-model.md) also applies. The workspace trust rules below take precedence
when deciding which inputs are trusted. Browser integrations have a separate
[playground model](playground-threat-model.md).

## Trust boundaries and assumptions

- **Attacker-controlled:** all inputs analyzed in an untrusted workspace, including source code,
    project configuration, and notebooks.
- **Trusted local input:** the editor, its extensions and their bundled files, the local machine and
    its file system, and the user's configuration and trust decisions.

ty treats workspaces as trusted unless `untrustedWorkspace` is true at initialization. Trust extends
to everything in the workspace, including source code, configuration, notebooks, and the targets of
symlinks that point outside it.

## Security invariants

- **Code Execution:** In untrusted workspaces, the editor extension and server may launch executables
    bundled with the extension or trusted programs already installed on the host. They must not
    execute workspace or dependency code, including code run during installation.
- **Edits:** Only editing operations may change source files, and they must not change unrelated
    files.
- **Output:** Attacker-controlled text in diagnostics, documentation, and logs must not inject code
    or editor commands. Actions intentionally provided by the editor or language server may run when
    the user chooses them.
- **Availability:** An isolated analysis failure or slow analysis is a correctness or performance bug.
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
