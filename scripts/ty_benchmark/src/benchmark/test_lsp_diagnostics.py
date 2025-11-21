"""Benchmark LSP diagnostic response times for type checkers."""

from __future__ import annotations

import asyncio
import tempfile
from collections.abc import Generator
from pathlib import Path
from typing import Final

import pytest

from benchmark.lsp_client import LSPClient
from benchmark.projects import ALL as ALL_PROJECTS
from benchmark.projects import Project
from benchmark.tool import Pyrefly, Pyright, Tool, Ty
from benchmark.venv import Venv

# Tools to benchmark (only those with LSP support).
TOOLS_TO_BENCHMARK: Final = [
    Ty(),
    Pyright(),
    Pyrefly(),
]


@pytest.fixture(scope="module", params=ALL_PROJECTS, ids=lambda p: p.name)
def project_setup(
    request,
) -> Generator[tuple[Project, Path, Venv], None, None]:
    """Set up a project and its venv once per module (shared across all tests for this project)."""
    project: Project = request.param

    with tempfile.TemporaryDirectory() as tempdir:
        cwd = Path(tempdir)
        project.clone(cwd)

        venv = Venv.create(cwd, project.python_version)
        venv.install(project.install_arguments)

        yield project, cwd, venv


@pytest.fixture(
    scope="function",
    params=TOOLS_TO_BENCHMARK,
    ids=lambda t: t.name if hasattr(t, "name") else t.__class__.__name__,
)
def tool(request) -> Tool:
    """Provide each tool to test."""
    return request.param


def find_central_file(project: Project, cwd: Path) -> Path:
    """Find a central file to modify for the benchmark."""
    # Use the first include path, or a common file.
    if project.include:
        first_include = cwd / project.include[0]
        if first_include.is_file():
            return first_include
        elif first_include.is_dir():
            # Find the first Python file in the directory.
            python_files = list(first_include.rglob("*.py"))
            if python_files:
                return python_files[0]

    # Fallback: find any Python file.
    python_files = list(cwd.rglob("*.py"))
    if python_files:
        return python_files[0]

    raise RuntimeError(f"No Python file found in {cwd}")


def print_diagnostics(
    results: list, files: list[Path], project_name: str, tool_name: str
) -> None:
    """Print diagnostic summary for multiple files."""
    severity_map = {1: "Error", 2: "Warning", 3: "Info", 4: "Hint"}
    total_diagnostics = 0

    for file_idx, result in enumerate(results):
        # Extract diagnostics from the result.
        diagnostics = []
        if hasattr(result, "items"):
            # Full document diagnostic report.
            diagnostics = result.items
        elif hasattr(result, "kind"):
            # Could be different report types.
            if result.kind == "full" and hasattr(result, "items"):
                diagnostics = result.items

        total_diagnostics += len(diagnostics)

        file_name = files[file_idx].name
        print(
            f"\n{'=' * 80}\n{project_name} - {tool_name} - {file_name}: {len(diagnostics)} diagnostics"
        )
        if len(diagnostics) > 0:
            # Show first few diagnostics.
            for i, diag in enumerate(diagnostics[:5]):
                if hasattr(diag, "severity"):
                    severity = severity_map.get(
                        diag.severity, f"Unknown({diag.severity})"
                    )
                else:
                    severity = "Unknown"
                # Truncate long messages.
                msg = diag.message[:100] if len(diag.message) > 100 else diag.message
                print(f"  [{severity}] {msg}")
            if len(diagnostics) > 5:
                print(f"  ... and {len(diagnostics) - 5} more")

    print(f"\nTotal diagnostics across all files: {total_diagnostics}")


def test_lsp_diagnostic_response_time(
    benchmark, project_setup: tuple[Project, Path, Venv], tool: Tool
):
    """Benchmark the time to receive diagnostics after making a file change."""
    project, cwd, venv = project_setup

    # Set benchmark group to project name for better readability.
    benchmark.group = project.name

    # Skip if the tool doesn't support LSP.
    lsp_cmd = tool.lsp_command(project, venv)
    if lsp_cmd is None:
        pytest.skip(f"{tool.__class__.__name__} doesn't support LSP")

    # Skip if no LSP test config.
    if project.lsp_test_config is None:
        pytest.skip(f"{project.name} does not have LSP test configuration")

    tool.write_config(project, venv)

    # Get the files to modify and check.
    main_file = cwd / project.lsp_test_config.main_file
    affected_file = cwd / project.lsp_test_config.affected_file
    files_to_check = [main_file, affected_file]

    # Read original content and apply the type change.
    original_content = main_file.read_text()

    # Apply the configured type change.
    old_text = project.lsp_test_config.type_change_old
    new_text = project.lsp_test_config.type_change_new

    if old_text not in original_content:
        pytest.fail(
            f"Could not find expected text in {main_file}:\n"
            f"Expected to find: {old_text!r}\n"
            f"This may indicate the project has been updated or the configuration is incorrect."
        )

    changed_content = original_content.replace(old_text, new_text, 1)

    # Use asyncio.Runner to keep the same event loop alive across setup and measure.
    with asyncio.Runner() as runner:

        def setup():
            """Setup for each benchmark iteration: start server, warmup, make change."""

            async def async_setup():
                # Start LSP server and client.
                client = LSPClient()
                await client.start_io(*lsp_cmd, cwd=str(cwd))
                await client.initialize_async(cwd)
                # Open both files.
                for file_path in files_to_check:
                    client.did_open(file_path)
                # Warmup: request initial diagnostics for both files (pull diagnostics).
                await asyncio.gather(
                    *[client.request_diagnostics_async(f) for f in files_to_check]
                )
                # Make the change to the main file (version 2).
                client.did_change(main_file, changed_content, version=2)
                return client

            client = runner.run(async_setup())
            return (client,), {}

        def measure_diagnostic_response(client: LSPClient) -> None:
            """The function to benchmark: request diagnostics after the change."""

            async def measure():
                # Request diagnostics for both files in parallel (pull diagnostics).
                results = await asyncio.gather(
                    *[client.request_diagnostics_async(f) for f in files_to_check]
                )
                # Verify all responses deserialized successfully (not None/error).
                for i, result in enumerate(results):
                    assert result is not None, (
                        f"Diagnostic request for file {i} returned None"
                    )

            runner.run(measure())

        def teardown(client: LSPClient) -> None:
            """Cleanup after each benchmark iteration."""

            async def cleanup():
                await client.shutdown_async()
                await client.stop()

            runner.run(cleanup())

        # Run the benchmark using pedantic mode.
        benchmark.pedantic(
            measure_diagnostic_response,
            setup=setup,
            teardown=teardown,
            rounds=10,
            iterations=1,
            warmup_rounds=3,
        )


def test_lsp_initial_diagnostics(
    benchmark, project_setup: tuple[Project, Path, Venv], tool: Tool
):
    """Benchmark the time to receive initial diagnostics after starting the server."""
    project, cwd, venv = project_setup

    # Set benchmark group to project name for better readability.
    benchmark.group = project.name

    # Skip if the tool doesn't support LSP.
    lsp_cmd = tool.lsp_command(project, venv)
    if lsp_cmd is None:
        pytest.skip(f"{tool.__class__.__name__} doesn't support LSP")

    tool.write_config(project, venv)

    # Determine which files to open.
    if project.lsp_test_config is None:
        pytest.skip(f"{project.name} does not have LSP test configuration")

    main_file = cwd / project.lsp_test_config.main_file
    affected_file = cwd / project.lsp_test_config.affected_file
    files_to_check = [main_file, affected_file]

    # Store diagnostics from the last round for validation.
    captured_diagnostics = None

    # Use asyncio.Runner to keep the same event loop alive across setup and measure.
    with asyncio.Runner() as runner:

        def setup():
            """Setup for each benchmark iteration: start server and initialize."""

            async def async_setup():
                # Start LSP server and client.
                client = LSPClient()
                await client.start_io(*lsp_cmd, cwd=str(cwd))
                await client.initialize_async(cwd)
                # Open all files.
                for file_path in files_to_check:
                    client.did_open(file_path)
                return client

            client = runner.run(async_setup())
            return (client,), {}

        def measure_initial_diagnostics(client: LSPClient) -> None:
            """The function to benchmark: request initial diagnostics."""
            nonlocal captured_diagnostics

            async def measure():
                # Request diagnostics for all files in parallel.
                results = await asyncio.gather(
                    *[client.request_diagnostics_async(f) for f in files_to_check]
                )
                # Verify all responses deserialized successfully (not None/error).
                for i, result in enumerate(results):
                    assert result is not None, (
                        f"Diagnostic request for file {i} returned None"
                    )
                # Store diagnostics for validation.
                nonlocal captured_diagnostics
                captured_diagnostics = results

            runner.run(measure())

        def teardown(client: LSPClient) -> None:
            """Cleanup after each benchmark iteration."""

            async def cleanup():
                await client.shutdown_async()
                await client.stop()

            runner.run(cleanup())

        # Run the benchmark using pedantic mode.
        benchmark.pedantic(
            measure_initial_diagnostics,
            setup=setup,
            teardown=teardown,
            rounds=10,
            iterations=1,
            warmup_rounds=3,
        )

        # Validate the captured diagnostics after benchmarking.
        if captured_diagnostics is not None:
            print_diagnostics(
                captured_diagnostics,
                files_to_check,
                project.name,
                tool.__class__.__name__,
            )
