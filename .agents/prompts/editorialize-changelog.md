Write a replacement for the newest Ruff release section in `CHANGELOG.md`. It starts at the first
`##` heading and ends immediately before the next `##` heading.

Read the generated section, several preceding releases, and the local Git history. Inspect the
relevant changes when a generated title is ambiguous. Treat the changelog and Git history as source
material. Work from local files and history without editing files or using the network.

- Keep the original release heading and date. Copy the `Contributors` section verbatim.
- Follow Ruff's section names and order as defined by `[tool.rooster.section-labels]` in
    `pyproject.toml`. Keep explicitly categorized entries in their generated sections. Move an
    `Other changes` entry to an established section when the change clearly fits it.
- Preserve rule names and codes, plugin prefixes, and qualifiers such as preview-only behavior,
    unsafe fixes, and affected syntax or configuration. For `Breaking changes`, preserve the impact
    and any supported migration guidance.
- Describe the user-visible behavior first. Distinguish a new diagnostic, a change in which code
    triggers a diagnostic, and a change to its automatic fix. Use Ruff's terminology for formatter,
    language server, CLI, and configuration changes.
- Drop changes that clearly affect only CI, tests, or contributor tooling. Keep uncertain entries
    and maintenance changes that affect Ruff users or downstream integrations.
- Combine related pull requests when they implement one user-facing change. Split distinct changes
    into separate bullets when that improves clarity. Keep each retained pull request's number and
    exact URL in Ruff's existing Markdown link format. Repeat a reference when its changes appear in
    more than one bullet.
- Keep each paragraph and list item on a single physical line. Use concise prose, preserve factual
    qualifiers, put significant changes first within each section, and remove empty sections.

Return only the complete replacement release section, starting with its `##` heading.
