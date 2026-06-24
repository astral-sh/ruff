"""Simple LSP client for benchmarking diagnostic response times."""

import asyncio
import logging
from asyncio import Future
from pathlib import Path
from typing import Any, NamedTuple, override

from lsprotocol import types as lsp
from pygls.lsp.client import LanguageClient


def _register_notebook_structure_hooks(converter):
    """Register structure hooks for notebook document types to work around cattrs deserialization issues."""

    # Define a union type that cattrs struggles with.
    notebook_filter_union = (
        str
        | lsp.NotebookDocumentFilterNotebookType
        | lsp.NotebookDocumentFilterScheme
        | lsp.NotebookDocumentFilterPattern
        | None
    )

    def structure_notebook_filter(obj: Any, _type):
        """Structure a notebook filter field from various possible types."""
        if obj is None:
            return None
        if isinstance(obj, str):
            return obj
        if isinstance(obj, dict):
            # Try to structure it as one of the known types.
            if "notebookType" in obj:
                return converter.structure(obj, lsp.NotebookDocumentFilterNotebookType)
            elif "scheme" in obj:
                return converter.structure(obj, lsp.NotebookDocumentFilterScheme)
            elif "pattern" in obj:
                return converter.structure(obj, lsp.NotebookDocumentFilterPattern)
        return obj

    converter.register_structure_hook(notebook_filter_union, structure_notebook_filter)


class LSPClient(LanguageClient):
    """A minimal LSP client for benchmarking purposes."""

    server_capabilities: lsp.ServerCapabilities
    diagnostics: dict[str, Future[lsp.PublishDiagnosticsParams]]

    def __init__(
        self,
    ):
        super().__init__(
            "ty_benchmark",
            "v1",
        )

        # Register custom structure hooks to work around lsprotocol/cattrs issues.
        _register_notebook_structure_hooks(self.protocol._converter)

        self.diagnostics = {}

        @self.feature(lsp.TEXT_DOCUMENT_PUBLISH_DIAGNOSTICS)
        def publish_diagnostics(
            client: LSPClient, params: lsp.PublishDiagnosticsParams
        ):
            logging.info(
                f"Received publish_diagnostics for {params.uri} with version={params.version}, diagnostics count={len(params.diagnostics)}"
            )
            future = self.diagnostics.get(params.uri, None)

            if future is None or future.done():
                future = asyncio.Future()
                self.diagnostics[params.uri] = future

            future.set_result(params)

        @self.feature(lsp.WINDOW_LOG_MESSAGE)
        def log_message(client: LSPClient, params: lsp.LogMessageParams):
            if params.type == lsp.MessageType.Error:
                logging.error(f"server error: {params.message}")
            elif params.type == lsp.MessageType.Warning:
                logging.warning(f"server warning: {params.message}")
            else:
                logging.info(f"server info: {params.message}")

    @override
    async def initialize_async(
        self, params: lsp.InitializeParams
    ) -> lsp.InitializeResult:
        result = await super().initialize_async(params)

        self.server_capabilities = result.capabilities

        logging.info(
            f"Pull diagnostic support: {self.server_supports_pull_diagnostics}"
        )

        return result

    def clear_pending_publish_diagnostics(self):
        self.diagnostics.clear()

    @property
    def server_supports_pull_diagnostics(self) -> bool:
        diagnostic_provider = self.server_capabilities.diagnostic_provider

        return diagnostic_provider is not None

    async def text_document_diagnostics_async(self, path: Path) -> list[lsp.Diagnostic]:
        """
        Returns the diagnostics for `path`

        Uses pull diagnostics if the server supports it or waits for a publish diagnostics
        notification if not.
        """
        if self.server_supports_pull_diagnostics:
            pull_diagnostics = await self.text_document_diagnostic_async(
                lsp.DocumentDiagnosticParams(
                    text_document=lsp.TextDocumentIdentifier(uri=path.as_uri())
                )
            )

            assert isinstance(
                pull_diagnostics, lsp.RelatedFullDocumentDiagnosticReport
            ), "Expected a full diagnostic report"

            return list(pull_diagnostics.items)

        # Use publish diagnostics otherwise.
        # Pyrefly doesn't support pull diagnostics as of today (27th of November 2025)
        publish_diagnostics = await self.wait_for_push_diagnostics_async(path)
        return list(publish_diagnostics.diagnostics)

    async def text_documents_diagnostics_async(
        self, files: list[Path]
    ) -> list[FileDiagnostics]:
        responses = await asyncio.gather(
            *(self.text_document_diagnostics_async(f) for f in files)
        )

        return [
            FileDiagnostics(file, diagnostics=list(response))
            for file, response in zip(files, responses)
        ]

    async def wait_for_push_diagnostics_async(
        self, path: Path, timeout: float = 60
    ) -> lsp.PublishDiagnosticsParams:
        future = self.diagnostics.get(path.as_uri(), None)

        if future is None:
            future = asyncio.Future()
            self.diagnostics[path.as_uri()] = future

        try:
            logging.info(f"Waiting for push diagnostics for {path}")
            result = await asyncio.wait_for(future, timeout)
            logging.info(f"Awaited push diagnostics for {path}")
        finally:
            self.diagnostics.pop(path.as_uri())

        return result


class FileDiagnostics(NamedTuple):
    file: Path
    diagnostics: list[lsp.Diagnostic]
