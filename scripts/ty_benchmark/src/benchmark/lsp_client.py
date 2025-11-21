"""Simple LSP client for benchmarking diagnostic response times."""

from __future__ import annotations

import asyncio
from concurrent.futures import Future
from pathlib import Path
from typing import Any

from lsprotocol import types as lsp
from pygls.client import JsonRPCClient
from pygls.protocol import JsonRPCProtocol


class LSPProtocol(JsonRPCProtocol):
    """Custom protocol with notification waiting capability."""

    def __init__(self, *args: Any, **kwargs: Any):
        super().__init__(*args, **kwargs)
        self._notification_futures: dict[str, Future[Any]] = {}
        self.log_messages: list[str] = []

    def _handle_notification(self, method_name: str, params: Any) -> None:
        """Override to support notification futures."""
        # Capture log messages from the server.
        if method_name == "window/logMessage":
            self.log_messages.append(params.message if hasattr(params, "message") else str(params))

        # Check if there's a pending future for this notification.
        if method_name in self._notification_futures:
            future = self._notification_futures.pop(method_name)
            future.set_result(params)

        # Call the parent handler to execute registered feature handlers.
        super()._handle_notification(method_name, params)

    def wait_for_notification(self, method: str) -> Future[Any]:
        """Wait for a notification with the given method name."""
        future: Future[Any] = Future()
        self._notification_futures[method] = future
        return future

    def wait_for_notification_async(self, method: str) -> asyncio.Future[Any]:
        """Async version of wait_for_notification."""
        future = self.wait_for_notification(method)
        return asyncio.wrap_future(future)


class LSPClient(JsonRPCClient):
    """A minimal LSP client for benchmarking purposes."""

    def __init__(self):
        super().__init__(protocol_cls=LSPProtocol)

    async def initialize_async(
        self, root_uri: Path, initialization_options: dict[str, Any] | None = None
    ) -> lsp.InitializeResult:
        """Initialize the LSP server."""
        result = await self.protocol.send_request_async(
            lsp.INITIALIZE,
            lsp.InitializeParams(
                process_id=None,
                root_uri=root_uri.as_uri(),
                workspace_folders=[
                    lsp.WorkspaceFolder(uri=root_uri.as_uri(), name=root_uri.name)
                ],
                capabilities=lsp.ClientCapabilities(
                    text_document=lsp.TextDocumentClientCapabilities(
                        diagnostic=lsp.DiagnosticClientCapabilities(
                            dynamic_registration=False,
                            related_document_support=True,
                        )
                    )
                ),
                initialization_options=initialization_options,
            ),
        )
        self.protocol.notify(lsp.INITIALIZED, lsp.InitializedParams())

        # Send configuration via workspace/didChangeConfiguration.
        if initialization_options:
            self.protocol.notify(
                "workspace/didChangeConfiguration",
                {"settings": initialization_options},
            )

        return result

    def did_open(self, file_path: Path, language_id: str = "python") -> None:
        """Notify the server that a file was opened."""
        content = file_path.read_text()
        self.protocol.notify(
            lsp.TEXT_DOCUMENT_DID_OPEN,
            lsp.DidOpenTextDocumentParams(
                text_document=lsp.TextDocumentItem(
                    uri=file_path.as_uri(),
                    language_id=language_id,
                    version=1,
                    text=content,
                )
            ),
        )

    def did_change(self, file_path: Path, new_content: str, version: int) -> None:
        """Notify the server that a file was changed."""
        self.protocol.notify(
            lsp.TEXT_DOCUMENT_DID_CHANGE,
            lsp.DidChangeTextDocumentParams(
                text_document=lsp.VersionedTextDocumentIdentifier(
                    uri=file_path.as_uri(),
                    version=version,
                ),
                content_changes=[
                    lsp.TextDocumentContentChangeWholeDocument(text=new_content)
                ],
            ),
        )

    async def wait_for_diagnostics_async(
        self, timeout: float = 30.0
    ) -> lsp.PublishDiagnosticsParams:
        """Wait for diagnostics to be published (push diagnostics)."""
        assert isinstance(self.protocol, LSPProtocol)
        future = self.protocol.wait_for_notification_async(
            lsp.TEXT_DOCUMENT_PUBLISH_DIAGNOSTICS
        )
        return await asyncio.wait_for(future, timeout=timeout)

    async def request_diagnostics_async(
        self, file_path: Path, timeout: float = 30.0
    ) -> lsp.DocumentDiagnosticReport:
        """Request diagnostics for a file (pull diagnostics)."""
        result = await asyncio.wait_for(
            self.protocol.send_request_async(
                lsp.TEXT_DOCUMENT_DIAGNOSTIC,
                lsp.DocumentDiagnosticParams(
                    text_document=lsp.TextDocumentIdentifier(uri=file_path.as_uri())
                ),
            ),
            timeout=timeout,
        )
        return result

    async def shutdown_async(self) -> None:
        """Shutdown the LSP server."""
        await self.protocol.send_request_async(lsp.SHUTDOWN, None)
        self.protocol.notify(lsp.EXIT, None)
