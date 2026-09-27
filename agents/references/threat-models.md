# Security threat models

Use each model that covers the behavior under review.

- [Ruff and ty CLI](cli-threat-model.md): command-line tools, native libraries, analysis,
    configuration, file operations, subprocesses, and diagnostics.
- [Language servers](language-server-threat-model.md): editor messages, workspace trust, document
    access, edits, and logs.
- [Playgrounds](playground-threat-model.md): browser applications, WASM, Python execution, and the
    HTTP sharing API.
- [GitHub repository](repository-threat-model.md):
    CI, releases, publishing, ecosystem jobs, and maintainer automation.

Development tools that build or run third-party code follow the repository model.
