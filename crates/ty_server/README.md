# The ty Language Server

`ty server` implements the Language Server Protocol for ty's editor integrations.

## LSP extensions

This document describes ty's extensions to the Language Server Protocol. These extensions are a best-effort contract between the server and its clients; when in doubt, consult the implementation.

Client capabilities for these extensions are advertised through the `experimental` field of [`ClientCapabilities`](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#clientCapabilities).

### Full diagnostic output

Experimental client capability: `{ "fullDiagnosticOutput": boolean }`

When this capability is `true`, ty includes a human-readable multiline rendering of a diagnostic in the diagnostic's `data` field.

```ts
interface Diagnostic {
    // Standard LSP fields omitted.
    data?: {
        /** A human-readable multiline rendering of the diagnostic. */
        rendered?: string;

        /** The original ty diagnostic identifier, such as `invalid-argument-type`. */
        diagnostic_id?: string;

        // Other ty-specific fields may also be present.
    };
}
```

For diagnostics that support this extension, `rendered` and `diagnostic_id` are either both present or both absent. Clients may use `diagnostic_id` to preserve the original identifier if they replace `Diagnostic.code` with a link to the rendered output. Clients must preserve `Diagnostic.data` when returning a diagnostic in a `textDocument/codeAction` request so that code actions continue to work.
