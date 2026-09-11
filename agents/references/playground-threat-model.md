# Ruff and ty playground threat model

## Overview

This model covers browser analysis, Python execution, file exports, and the public sharing API. The
[CLI threat model](cli-threat-model.md) defines the general criteria for security findings.

Builds and publication have a separate
[repository threat model](repository-threat-model.md).

## Trust boundaries and assumptions

- **Attacker-controlled:** shared URLs and workspace contents, including source code, configuration,
    filenames, and which file is selected.
- **Trusted environment:** the browser, operating system, local caches, HTTPS, application
    dependencies including Pyodide, and configured Cloudflare service and KV namespace.
- **Python execution:** Choosing Run authorizes the workspace's Python code to use the page's
    JavaScript and browser APIs through Pyodide.
- **API callers:** The sharing API accepts anonymous uploads, creates a random identifier, and
    returns stored text to anyone who has that identifier.

## Security invariants

### Browser applications

- **Code Execution:** Analysis must not execute user-supplied code. Running Python requires choosing
    Run.
- **Rendering:** Diagnostics, documentation, filenames, errors, and program output must not allow
    attacker-controlled text to run code or invoke commands in the playground.
- **Paths and exports:** Loading or editing workspace files must not overwrite unrelated application
    state. Extracting an exported ZIP must not write outside the chosen directory.
- **Source privacy:** Analysis must not upload source. Share and the Markdown copy actions upload the
    selected workspace. Restoring a shared workspace must not disclose other playground data.
- **Availability:** An isolated analysis failure or slow analysis is a correctness or performance
    bug. A security issue requires repeatable, disproportionate resource use that materially
    disrupts the browser or user's machine beyond the playground tab.

### Sharing API

- **Stored values:** Requests must not overwrite unrelated entries, reveal their identifiers or
    contents, expose deployment credentials, or execute code in the worker.
- **Availability:** Anonymous requests must not cause disproportionate resource use that denies
    service to other users or exhausts shared resources.

## Severity calibration

- **Critical:** Service or host compromise, or loss of credentials with comparable authority,
    with few prerequisites.
- **High:** Substantial confidentiality or integrity harm, such as executing attacker code on
    opening a shared link and stealing other playground data.
- **Medium:** Limited disclosure or modification, or reliable resource exhaustion affecting the
    shared service.
- **Low:** A narrow safety gap with limited practical impact, or reliable resource exhaustion
    affecting a user's browser or machine beyond the playground tab.
- **Informational:** A genuine security concern with negligible current impact.
