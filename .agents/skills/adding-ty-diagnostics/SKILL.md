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

Rule renames usually need no new tests. Update existing test expectations to use the new name.
Skip tests that only demonstrate that the old name still works in configuration or ignore comments.
The [rule-selection tests](../../../crates/ty/tests/cli/rule_selection.rs) and
[suppression mdtests](../../../crates/ty_python_semantic/resources/mdtest/import/errors.md#suppressing-import-not-found-with-the-previous-rule-name)
already cover those cases.

When adding a ty rule that is disabled by default, normally enable it at `warn` in `.github/ty-ecosystem.toml` so the
ecosystem-analyzer workflows report its effects.

When declaring a new stable rule with `declare_lint!`, set `status` to `LintStatus::stable(<next version of ty>)`.
This also applies to a new rule split out of an existing rule.
You should determine what the next version of ty will be by inspecting https://pypi.org/pypi/ty/json, finding what
the latest release of ty is, and incrementing the patch version by one. For example, if the latest release of ty is `0.5.3`, the status should be `LintStatus::stable("0.5.4")`.

When renaming a rule, keep the version from its existing declaration.

Document changes such as renames and splits in a `## Rule history` section at the end of each
affected rule's documentation. Use a terse bullet for each change, starting with the ty version
that introduced it. For a rename, give the old and new names. For a split, name the original rule
and the rules split out of it. Follow the format of existing rule histories, such as the one for
[`invalid-implicit-return-in-function-with-stub-body`](../../../crates/ty_python_semantic/resources/lint_docs/invalid-implicit-return-in-function-with-stub-body.md#rule-history).
