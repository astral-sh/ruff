"""A runner for Markdown-based tests for Red Knot"""
# /// script
# requires-python = ">=3.11"
# dependencies = [
#     "rich",
#     "watchfiles",
# ]
# ///

from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path
from typing import Final, Literal, Never, assert_never

from rich.console import Console
from watchfiles import Change, watch

CRATE_NAME: Final = "red_knot_python_semantic"
CRATE_ROOT: Final = Path(__file__).resolve().parent
MDTEST_DIR: Final = CRATE_ROOT / "resources" / "mdtest"


class MDTestRunner:
    mdtest_executable: Path | None
    console: Console

    def __init__(self) -> None:
        self.mdtest_executable = None
        self.console = Console()

    def _run_cargo_test(self, *, message_format: Literal["human", "json"]) -> str:
        return subprocess.check_output(
            [
                "cargo",
                "test",
                "--package",
                CRATE_NAME,
                "--no-run",
                "--color=always",
                "--message-format",
                message_format,
            ],
            cwd=CRATE_ROOT,
            env=dict(os.environ, CLI_COLOR="1"),
            stderr=subprocess.STDOUT,
            text=True,
        )

    def _recompile_tests(
        self, status_message: str, *, message_on_success: bool = True
    ) -> bool:
        with self.console.status(status_message):
            # Run it with 'human' format in case there are errors:
            try:
                self._run_cargo_test(message_format="human")
            except subprocess.CalledProcessError as e:
                print(e.output)
                return False

            # Run it again with 'json' format to find the mdtest executable:
            try:
                json_output = self._run_cargo_test(message_format="json")
            except subprocess.CalledProcessError as _:
                # `cargo test` can still fail if something changed in between the two runs.
                # Here we don't have a human-readable output, so just show a generic message:
                self.console.print("[red]Error[/red]: Failed to compile tests")
                return False

            if json_output:
                self._get_executable_path_from_json(json_output)

        if message_on_success:
            self.console.print("[dim]Tests compiled successfully[/dim]")
        return True

    def _get_executable_path_from_json(self, json_output: str) -> None:
        for json_line in json_output.splitlines():
            try:
                data = json.loads(json_line)
            except json.JSONDecodeError:
                continue
            if data.get("target", {}).get("name") == "mdtest":
                self.mdtest_executable = Path(data["executable"])
                break
        else:
            raise RuntimeError(
                "Could not find mdtest executable after successful compilation"
            )

    def _run_mdtest(
        self, arguments: list[str] | None = None, *, capture_output: bool = False
    ) -> subprocess.CompletedProcess:
        assert self.mdtest_executable is not None

        arguments = arguments or []
        return subprocess.run(
            [self.mdtest_executable, *arguments],
            cwd=CRATE_ROOT,
            env=dict(os.environ, CLICOLOR_FORCE="1"),
            capture_output=capture_output,
            text=True,
            check=False,
        )

    def _run_mdtests_for_file(self, markdown_file: Path) -> None:
        path_mangled = (
            markdown_file.as_posix()
            .replace("/", "_")
            .replace("-", "_")
            .removesuffix(".md")
        )
        test_name = f"mdtest__{path_mangled}"

        output = self._run_mdtest(["--exact", test_name], capture_output=True)

        if output.returncode == 0:
            if "running 0 tests\n" in output.stdout:
                self.console.log(
                    f"[yellow]Warning[/yellow]: No tests were executed with filter '{test_name}'"
                )
            else:
                self.console.print(
                    f"Test for [bold green]{markdown_file}[/bold green] succeeded"
                )
        else:
            self.console.print()
            self.console.rule(
                f"Test for [bold red]{markdown_file}[/bold red] failed",
                style="gray",
            )
            self._print_trimmed_cargo_test_output(
                output.stdout + output.stderr, test_name
            )

    def _print_trimmed_cargo_test_output(self, output: str, test_name: str) -> None:
        # Skip 'cargo test' boilerplate at the beginning:
        lines = output.splitlines()
        start_index = 0
        for i, line in enumerate(lines):
            if f"{test_name} stdout" in line:
                start_index = i
                break

        for line in lines[start_index + 1 :]:
            if "MDTEST_TEST_FILTER" in line:
                continue
            if line.strip() == "-" * 50:
                # Skip 'cargo test' boilerplate at the end
                break

            print(line)

    def watch(self) -> Never:
        self._recompile_tests("Compiling tests...", message_on_success=False)
        self._run_mdtest()
        self.console.print("[dim]Ready to watch for changes...[/dim]")

        for changes in watch(CRATE_ROOT):
            new_md_files = set()
            changed_md_files = set()
            rust_code_has_changed = False

            for change, path_str in changes:
                path = Path(path_str)

                if path.suffix == ".rs":
                    rust_code_has_changed = True
                    continue

                if path.suffix != ".md":
                    continue

                try:
                    relative_path = Path(path).relative_to(MDTEST_DIR)
                except ValueError:
                    continue

                match change:
                    case Change.added:
                        # When saving a file, some editors (looking at you, Vim) might first
                        # save the file with a temporary name (e.g. `file.md~`) and then rename
                        # it to the final name. This creates a `deleted` and `added` change.
                        # We treat those files as `changed` here.
                        if (Change.deleted, path_str) in changes:
                            changed_md_files.add(relative_path)
                        else:
                            new_md_files.add(relative_path)
                    case Change.modified:
                        changed_md_files.add(relative_path)
                    case Change.deleted:
                        # No need to do anything when a Markdown test is deleted
                        pass
                    case _ as unreachable:
                        assert_never(unreachable)

            if rust_code_has_changed:
                if self._recompile_tests("Rust code has changed, recompiling tests..."):
                    self._run_mdtest()
            elif new_md_files:
                files = " ".join(file.as_posix() for file in new_md_files)
                self._recompile_tests(
                    f"New Markdown test [yellow]{files}[/yellow] detected, recompiling tests..."
                )

            for path in new_md_files | changed_md_files:
                self._run_mdtests_for_file(path)


def main() -> None:
    try:
        runner = MDTestRunner()
        runner.watch()
    except KeyboardInterrupt:
        print()


if __name__ == "__main__":
    main()
