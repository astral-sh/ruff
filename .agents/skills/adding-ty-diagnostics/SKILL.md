---
name: adding-ty-diagnostics
description: Use when a user says "add a ty rule", "add a ty diagnostic", "write this new ty diagnostic", "change a ty error message", "review ty diagnostics", or asks to add, update, or review ty checks, diagnostic messages, subdiagnostics, or concise output behavior.
---

# Adding Ty Diagnostics

Use this skill when adding a ty rule or adding, changing, or reviewing a ty diagnostic.

**Keep error messages concise.** Think about how the diagnostic will look on a narrow terminal screen.

Put extra detail in subdiagnostics or secondary annotations when that helps, but make sure the primary diagnostic is understandable on its own.

Always check that the diagnostic still makes sense when the user passes `--output-format=concise`.

If the error code is entirely new or if you have changed the documentation for the error code,
you will need to run `cargo dev generate-all` after making your changes, to update the generated schema for ty
and the generated `.md` documentation files.

Diagnostics should usually be tested using mdtests. If you are changing behaviour for an existing diagnostic,
you should usually add your tests to a pre-existing `.md` file; otherwise, it may be appropriate to add a new
`.md` file for your tests. A new ty diagnostic should normally have at least one snapshot test showing how ty renders
it. For changes to existing diagnostics, snapshot tests are only usually necessary for diagnostics that use secondary
annotations or subdiagnostics. If you want to add a snapshot, inline `# snapshot` comments are preferred over the legacy
`<!-- snapshot-annotations -->` directive.

When adding a ty rule that is disabled by default, normally enable it at `warn` in `.github/ty-ecosystem.toml` so the
ecosystem-analyzer workflows report its effects.

When using the `declare_lint!` macro, the `status` field should be set to `LintStatus::stable(<next version of ty>)`.
You should determine what the next version of ty will be by inspecting https://pypi.org/pypi/ty/json, finding what
the latest release of ty is, and incrementing the patch version by one. For example, if the latest release of ty is `0.5.3`, the status should be `LintStatus::stable("0.5.4")`.
