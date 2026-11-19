- Follow Ruff's section names and order as defined by `[tool.rooster.section-labels]` in
    `pyproject.toml`. Keep explicitly categorized entries in their generated sections.
- Preserve rule names and codes, plugin prefixes, and qualifiers such as preview-only behavior,
    unsafe fixes, and affected syntax or configuration.
- Distinguish a new diagnostic, a change in which code triggers a diagnostic, and a change to its
    automatic fix. Use Ruff's terminology for formatter, language server, CLI, and configuration
    changes.
