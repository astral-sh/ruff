# The ty Language Server

`ty server` implements the Language Server Protocol for ty's editor integrations.

Extensions to the protocol are documented in the [ty language server documentation](https://docs.astral.sh/ty/features/language-server/).

## Requests for closed documents

Document requests can target local files under a project root, configured import search paths, or
the bundled-stub cache without a preceding `textDocument/didOpen`. Explicit queries also accept
excluded files and use the existing Python fallback for unrecognized extensions. They do not mark
the target open or add it to project checking. Client contents remain authoritative while open.

All document request handlers use shared target preparation. Full semantic tokens and folding ranges
retain their notebook-cell handling; diagnostics retain the configured checking mode. Rename,
references, completion, and hierarchy requests retain their existing search scope and restrictions.
Hierarchy follow-up requests already use a separate session-wide path.

The following limitations need separate work:

- Closed notebooks need cell URI and position mappings. Unknown virtual documents need a source
    provider. Both remain unsupported; client-opened notebooks and virtual documents are supported.
- Freshness relies on existing client file-change notifications. Without dynamic watcher
    registration, ty installs no watchers; without relative patterns, external changes can be missed.
- Queries do not trigger environment synchronization or additional script discovery. Scripts outside
    existing discovery coverage may lack their dedicated dependency environment.
