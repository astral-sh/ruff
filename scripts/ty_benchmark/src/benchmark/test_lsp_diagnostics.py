"""
Benchmarks for LSP servers

When debugging test failures, run pytest with `-s -v --log-cli-level=DEBUG`
"""

import asyncio
import tempfile
from abc import ABC, abstractmethod
from collections import Counter
from collections.abc import Callable, Generator
from pathlib import Path
from typing import Any, Final, override

import pytest
from lsprotocol import types as lsp

from benchmark.lsp_client import FileDiagnostics, LSPClient
from benchmark.projects import ALL as ALL_PROJECTS
from benchmark.projects import IncrementalEdit, Project
from benchmark.tool import Pyrefly, Pyright, Tool, Ty
from benchmark.venv import Venv

# Tools to benchmark (only those with LSP support).
TOOLS_TO_BENCHMARK: Final = [
    Ty(),
    Pyright(),
    Pyrefly(),
]

SEVERITY_LABELS: Final = {1: "Error", 2: "Warning", 3: "Info", 4: "Hint"}


@pytest.fixture(scope="module", params=ALL_PROJECTS, ids=lambda p: p.name)
def project_setup(
    request,
) -> Generator[tuple[Project, Venv], None, None]:
    """Set up a project and its venv once per module (shared across all tests for this project)."""
    project: Project = request.param

    with tempfile.TemporaryDirectory() as tempdir:
        cwd = Path(tempdir)
        project.clone(cwd)

        venv = Venv.create(cwd, project.python_version)
        venv.install(project.install_arguments)

        yield project, venv


@pytest.fixture(
    scope="function",
    params=TOOLS_TO_BENCHMARK,
    ids=lambda t: t.name(),
)
def tool(request) -> Tool:
    """Provide each tool to test."""
    return request.param


def test_fetch_diagnostics(
    request, benchmark, project_setup: tuple[Project, Venv], tool: Tool
):
    """Benchmark the time to receive initial diagnostics after starting the server."""

    project, venv = project_setup

    run_benchmark(
        request,
        benchmark,
        project,
        tool,
        venv,
        FetchDiagnostics,
    )


def test_incremental_edit(
    request, benchmark, project_setup: tuple[Project, Venv], tool: Tool
):
    """Benchmark the time to receive diagnostics after making an edit to a file."""

    project, venv = project_setup

    run_benchmark(request, benchmark, project, tool, venv, IncrementalEditTest)


def run_benchmark(
    request: Any,
    benchmark: Any,
    project: Project,
    tool: Tool,
    venv: Venv,
    init_test: Callable[[Project, Tool, Venv], LspTest],
):
    # Set benchmark group to project name for better readability.
    benchmark.group = project.name
    verbose = request.config.getoption("verbose") > 0

    edited_file_backup: Path | None = None

    # some make changes to the main file. Create a backup and restore it before each test
    # and once the entire suite is done.
    if project.edit:
        edited_file_path = venv.project_path / project.edit.edited_file
        edited_file_backup = edited_file_path.with_name(edited_file_path.name + ".bak")
        edited_file_path.copy(edited_file_backup)

    try:
        tool.write_config(project, venv)

        # Use asyncio.Runner to keep the same event loop alive across setup and measure.
        with asyncio.Runner() as runner:

            def setup():
                if edited_file_backup:
                    edited_file_backup.copy(edited_file_backup.with_suffix(""))

                test = init_test(project, tool, venv)

                runner.run(test.setup())
                return (test,), {}

            def run(test: LspTest) -> None:
                runner.run(test.run())

            def teardown(test: LspTest) -> None:
                nonlocal verbose

                test.assert_output(verbose=verbose)
                runner.run(test.teardown())

                # Only do verbose output on the first (warm-up) run to avoid
                # that the printing changes the benchmark result.
                verbose = False

            # Run the benchmark using pedantic mode.
            benchmark.pedantic(
                run,
                setup=setup,
                teardown=teardown,
                # total executions = rounds * iterations
                warmup_rounds=2,
                rounds=10,
                iterations=1,
            )
    finally:
        if edited_file_backup:
            edited_file_backup.copy(edited_file_backup.with_suffix(""))


class LspTest(ABC):
    client: LSPClient
    venv: Venv
    project: Project
    tool: Tool
    edit: IncrementalEdit

    def __init__(self, project: Project, tool: Tool, venv: Venv):
        edit = project.edit
        if not edit:
            pytest.skip(f"{project.name} does not have an incremental edit")
            return

        self.project = project
        self.venv = venv
        self.tool = tool
        self.client = LSPClient()
        self.edit = edit

    @property
    def cwd(self) -> Path:
        return self.venv.project_path

    @property
    def edited_file_path(self) -> Path:
        return self.absolute_file_path(self.edit.edited_file)

    def absolute_file_path(self, file_path: str) -> Path:
        return self.cwd / file_path

    def files_to_check(self) -> Generator[Path, None, None]:
        yield self.edited_file_path

        for file in self.edit.affected_files:
            yield self.absolute_file_path(file)

    def open_all_files(self):
        for file_path in self.files_to_check():
            self.open_file(file_path)

    def open_file(self, path: Path):
        self.client.text_document_did_open(
            lsp.DidOpenTextDocumentParams(
                text_document=lsp.TextDocumentItem(
                    uri=path.as_uri(),
                    language_id="python",
                    version=1,
                    text=path.read_text(),
                )
            )
        )

    async def initialize(self):
        lsp_cmd = self.tool.lsp_command(self.project, self.venv)
        if lsp_cmd is None:
            pytest.skip(f"{self.tool.name()} doesn't support LSP")
            return

        await self.client.start_io(*lsp_cmd, cwd=self.cwd)

        await self.client.initialize_async(
            lsp.InitializeParams(
                root_uri=self.cwd.as_uri(),
                workspace_folders=[
                    lsp.WorkspaceFolder(uri=self.cwd.as_uri(), name=self.cwd.name)
                ],
                capabilities=lsp.ClientCapabilities(
                    text_document=lsp.TextDocumentClientCapabilities(
                        diagnostic=lsp.DiagnosticClientCapabilities(
                            data_support=True, dynamic_registration=False
                        ),
                        synchronization=lsp.TextDocumentSyncClientCapabilities(
                            did_save=True,
                        ),
                    ),
                ),
            ),
        )

        self.client.initialized(lsp.InitializedParams())

    @abstractmethod
    async def setup(self): ...

    @abstractmethod
    async def run(self): ...

    @abstractmethod
    def assert_output(self, verbose=False): ...

    async def teardown(self):
        await self.client.shutdown_async(None)
        self.client.exit(None)
        await self.client.stop()


class FetchDiagnostics(LspTest):
    diagnostics: list[FileDiagnostics] | None = None

    @override
    async def setup(self):
        await self.initialize()
        self.open_all_files()

    @override
    async def run(self):
        self.diagnostics = await self.client.text_documents_diagnostics_async(
            list(self.files_to_check())
        )

    @override
    def assert_output(self, verbose=False):
        if self.diagnostics is None:
            pytest.fail("No diagnostics were fetched")
            return

        if verbose:
            for file, diagnostics in self.diagnostics:
                if diagnostics:
                    print_diagnostics(file, diagnostics, self.venv.project_path)


class IncrementalEditTest(LspTest):
    before_edit_diagnostics: list[FileDiagnostics] | None = None
    after_edit_diagnostics: list[FileDiagnostics] | None = None
    new_content: str

    def __init__(self, project: Project, tool: Tool, venv: Venv):
        super().__init__(project, tool, venv)
        new_content = self.edit.apply_to(self.edited_file_path.read_text())

        if new_content is None:
            pytest.fail(
                f"Could not find expected text in {self.edited_file_path}:\n"
                f"Expected to find: {self.edit.replace_text}\n"
                f"This may indicate the project has been updated or the configuration is incorrect."
            )
            return

        self.new_content = new_content

    @override
    async def setup(self):
        await self.initialize()

        self.open_all_files()

        self.before_edit_diagnostics = (
            await self.client.text_documents_diagnostics_async(
                list(self.files_to_check())
            )
        )

        if not self.client.server_supports_pull_diagnostics:
            # Pyrefly sometimes sends more than one publish diagnostic per file,
            # and it doesn't support versioned publish diagnostics, making it impossible
            # for the client to tell if we already received the newest publish diagnostic
            # notification or not. Because of that, sleep, clear all publish diagnostic
            # notifications before sending the change notification.
            await asyncio.sleep(1)
            self.client.clear_pending_publish_diagnostics()

    @override
    async def run(self):
        self.client.text_document_did_change(
            lsp.DidChangeTextDocumentParams(
                text_document=lsp.VersionedTextDocumentIdentifier(
                    uri=self.edited_file_path.as_uri(),
                    version=2,
                ),
                content_changes=[
                    lsp.TextDocumentContentChangeWholeDocument(text=self.new_content)
                ],
            ),
        )

        all_files = list(self.files_to_check())

        # wait for the didChange publish notifications or pull the new diagnostics
        self.after_edit_diagnostics = (
            await self.client.text_documents_diagnostics_async(all_files)
        )

        after_did_change_sum = sum(
            len(diagnostics) for f, diagnostics in self.after_edit_diagnostics
        )

        # IMPORTANT: Write the file back to disk!
        # Pyrefly, as of Nov 27, requires that the content on disk
        # is updated to show cross-file diagnostics.
        self.edited_file_path.write_text(self.new_content)

        self.client.text_document_did_save(
            lsp.DidSaveTextDocumentParams(
                text_document=lsp.TextDocumentIdentifier(
                    uri=self.edited_file_path.as_uri(),
                ),
            )
        )

        # Pyrefly only publishes cross-file diagnostics after did_save.
        if isinstance(self.tool, Pyrefly):
            after_did_save_sum = after_did_change_sum

            # Pyrefly sometimes publishes multiple publish diagnostics after a `didSave`.
            # Especially if checking takes long, as it, e.g., is the case for homeassistant.
            # We need to wait until pyrefly sends us the cross-file diagnostics.
            # For now, we use a very simple heuristics where we simply check if the diagnostic
            # count between the `didChange` (not cross-file) and `didSave` (cross-file) is different.
            while after_did_save_sum == after_did_change_sum:
                self.after_edit_diagnostics = (
                    await self.client.text_documents_diagnostics_async(all_files)
                )

                after_did_save_sum = sum(
                    len(diagnostics) for f, diagnostics in self.after_edit_diagnostics
                )

    @override
    def assert_output(self, verbose=False):
        assert self.before_edit_diagnostics is not None, (
            "The before edit diagnostics should be initialized. Did you forget to call `setup`?"
        )
        assert self.after_edit_diagnostics is not None, (
            "The after edit diagnostics should be initialized if the test ran at least once. Did you forget to call `run`?"
        )

        before_edit_count = sum(
            len(diagnostics) for _, diagnostics in self.before_edit_diagnostics
        )

        after_edit_count = sum(
            len(diagnostics) for _, diagnostics in self.after_edit_diagnostics
        )

        assert after_edit_count > before_edit_count, (
            f"Expected more diagnostics after the change. "
            f"Initial: {before_edit_count}, After change: {after_edit_count}"
        )

        if verbose:
            print_diagnostic_diff(
                self.before_edit_diagnostics,
                self.after_edit_diagnostics,
                self.project.name,
                self.tool.name(),
                self.venv.project_path,
            )


def print_diagnostics(
    file: Path, diagnostics: list[lsp.Diagnostic], cwd: Path, label: str | None = None
):
    file = file.relative_to(cwd)

    if label:
        print(f"\n{file}: {len(diagnostics)} {label}")
    else:
        print(f"\n{file}: {len(diagnostics)} diagnostics")

    for diag in diagnostics:
        severity = SEVERITY_LABELS.get(diag.severity, f"Unknown({diag.severity})")
        print(
            f"{file}:{diag.range.start.line + 1}:{diag.range.start.character + 1} [{severity}] {diag.message}"
        )


type DiagnosticSignature = tuple[Path, str | int | None, str]


def diagnostic_signature(file: Path, diagnostic: lsp.Diagnostic) -> DiagnosticSignature:
    """Create a signature for a diagnostic based on file, code, and message (ignoring position)."""
    return (file, diagnostic.code, diagnostic.message)


def count_diagnostic_signatures(
    diagnostics_list: list[FileDiagnostics],
) -> Counter[DiagnosticSignature]:
    """Count occurrences of each diagnostic signature."""
    return Counter(
        diagnostic_signature(file, diagnostic)
        for file, diagnostics in diagnostics_list
        for diagnostic in diagnostics
    )


def diff_diagnostics(
    old: list[FileDiagnostics],
    new: list[FileDiagnostics],
) -> list[FileDiagnostics]:
    """Return diagnostics in `new` that aren't in `old`."""
    diff_counts = count_diagnostic_signatures(new) - count_diagnostic_signatures(old)

    result: list[FileDiagnostics] = []
    for file, diagnostics in new:
        new_for_file = []
        for d in diagnostics:
            sig = diagnostic_signature(file, d)
            if diff_counts[sig] > 0:
                diff_counts[sig] -= 1
                new_for_file.append(d)
        if new_for_file:
            result.append(FileDiagnostics(file, new_for_file))

    return result


def print_diagnostic_diff(
    before_diagnostics: list[FileDiagnostics],
    after_diagnostics: list[FileDiagnostics],
    project_name: str,
    tool_name: str,
    cwd: Path,
) -> None:
    """Print the difference in diagnostics before and after a change."""

    added = diff_diagnostics(before_diagnostics, after_diagnostics)
    removed = diff_diagnostics(after_diagnostics, before_diagnostics)

    total_added = sum(len(diagnostics) for _, diagnostics in added)
    total_removed = sum(len(diagnostics) for _, diagnostics in removed)

    print(f"\n{'=' * 80}")
    print(f"Diagnostic Diff: {project_name} - {tool_name}")
    print(f"{'=' * 80}")
    print(f"Added:   {total_added} diagnostic(s)")
    print(f"Removed: {total_removed} diagnostic(s)")

    for file, diagnostics in added:
        print_diagnostics(file, diagnostics, cwd, "added")

    for file, diagnostics in removed:
        print_diagnostics(file, diagnostics, cwd, "removed")

    print(f"{'=' * 80}")
