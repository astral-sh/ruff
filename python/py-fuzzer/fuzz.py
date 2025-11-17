"""
Run a Ruff executable on randomly generated (but syntactically valid)
Python source-code files.

This script can be installed into a virtual environment using
`uv pip install -e ./python/py-fuzzer` from the Ruff repository root,
or can be run using `uv run --project=./python/py-fuzzer fuzz`
(in which case the virtual environment does not need to be activated).
Note that using `uv run --project` rather than `uvx --from` means that
uv will respect the script's lockfile.

Example invocations of the script using `uv`:
- Run the fuzzer on Ruff's parser using seeds 0, 1, 2, 78 and 93 to generate the code:
  `uv run --project=./python/py-fuzzer fuzz --bin ruff 0-2 78 93`
- Run the fuzzer concurrently using seeds in range 0-10 inclusive,
  but only reporting bugs that are new on your branch:
  `uv run --project=./python/py-fuzzer fuzz --bin ruff 0-10 --only-new-bugs`
- Run the fuzzer concurrently on 10,000 different Python source-code files,
  using a random selection of seeds, and only print a summary at the end
  (the `shuf` command is Unix-specific):
  `uv run --project=./python/py-fuzzer fuzz --bin ruff $(shuf -i 0-1000000 -n 10000) --quiet
"""

from __future__ import annotations

import argparse
import ast
import concurrent.futures
import enum
import subprocess
import tempfile
from collections.abc import Callable
from dataclasses import KW_ONLY, dataclass
from functools import partial
from pathlib import Path
from typing import NewType, NoReturn, assert_never

from pysource_codegen import generate as generate_random_code
from pysource_minimize import CouldNotMinimize, minimize as minimize_repro
from rich_argparse import RawDescriptionRichHelpFormatter
from termcolor import colored

MinimizedSourceCode = NewType("MinimizedSourceCode", str)
Seed = NewType("Seed", int)
ExitCode = NewType("ExitCode", int)


def ty_contains_bug(code: str, *, ty_executable: Path) -> bool:
    """Return `True` if the code triggers a panic in type-checking code."""
    with tempfile.TemporaryDirectory() as tempdir:
        input_file = Path(tempdir, "input.py")
        input_file.write_text(code)
        completed_process = subprocess.run(
            [ty_executable, "check", input_file], capture_output=True, text=True
        )
    return completed_process.returncode not in {0, 1, 2}


def ruff_contains_bug(code: str, *, ruff_executable: Path) -> bool:
    """Return `True` if the code triggers a parser error."""
    completed_process = subprocess.run(
        [
            ruff_executable,
            "check",
            "--config",
            "lint.select=[]",
            "--no-cache",
            "--target-version",
            "py314",
            "--preview",
            "-",
        ],
        capture_output=True,
        text=True,
        input=code,
    )
    return completed_process.returncode != 0


def contains_bug(code: str, *, executable: Executable, executable_path: Path) -> bool:
    """Return `True` if the code triggers an error."""
    match executable:
        case Executable.RUFF:
            return ruff_contains_bug(code, ruff_executable=executable_path)
        case Executable.TY:
            return ty_contains_bug(code, ty_executable=executable_path)
        case _ as unreachable:
            assert_never(unreachable)


def contains_new_bug(
    code: str,
    *,
    executable: Executable,
    test_executable_path: Path,
    baseline_executable_path: Path,
) -> bool:
    """Return `True` if the code triggers a *new* parser error.

    A "new" parser error is one that exists with `test_executable`,
    but did not exist with `baseline_executable`.
    """
    return contains_bug(
        code, executable=executable, executable_path=test_executable_path
    ) and not contains_bug(
        code, executable=executable, executable_path=baseline_executable_path
    )


@dataclass(slots=True)
class FuzzResult:
    # The seed used to generate the random Python file.
    # The same seed always generates the same file.
    seed: Seed
    # If we found a bug, this will be the minimum Python code
    # required to trigger the bug. If not, it will be `None`.
    maybe_bug: MinimizedSourceCode | None
    # The executable we're testing
    executable: Executable
    _: KW_ONLY
    only_new_bugs: bool

    def print_description(self, index: int, num_seeds: int) -> None:
        """Describe the results of fuzzing the parser with this seed."""
        progress = f"[{index}/{num_seeds}]"
        msg = (
            colored(f"Ran fuzzer on seed {self.seed}", "red")
            if self.maybe_bug
            else colored(f"Ran fuzzer successfully on seed {self.seed}", "green")
        )
        print(f"{msg:<60} {progress:>15}", flush=True)

        new = "new " if self.only_new_bugs else ""

        if self.maybe_bug:
            match self.executable:
                case Executable.RUFF:
                    panic_message = f"The following code triggers a {new}parser bug:"
                case Executable.TY:
                    panic_message = f"The following code triggers a {new}ty panic:"
                case _ as unreachable:
                    assert_never(unreachable)

            print(colored(panic_message, "red"))
            print()
            print(self.maybe_bug)
            print(flush=True)


def fuzz_code(seed: Seed, args: ResolvedCliArgs) -> FuzzResult:
    """Return a `FuzzResult` instance describing the fuzzing result from this seed."""
    code = generate_random_code(seed)
    bug_found = False
    minimizer_callback: Callable[[str], bool] | None = None

    if args.baseline_executable_path is None:
        only_new_bugs = False
        if contains_bug(
            code, executable=args.executable, executable_path=args.test_executable_path
        ):
            bug_found = True
            minimizer_callback = partial(
                contains_bug,
                executable=args.executable,
                executable_path=args.test_executable_path,
            )
    else:
        only_new_bugs = True
        if contains_new_bug(
            code,
            executable=args.executable,
            test_executable_path=args.test_executable_path,
            baseline_executable_path=args.baseline_executable_path,
        ):
            bug_found = True
            minimizer_callback = partial(
                contains_new_bug,
                executable=args.executable,
                test_executable_path=args.test_executable_path,
                baseline_executable_path=args.baseline_executable_path,
            )

    if not bug_found:
        return FuzzResult(seed, None, args.executable, only_new_bugs=only_new_bugs)

    assert minimizer_callback is not None

    try:
        maybe_bug = MinimizedSourceCode(minimize_repro(code, minimizer_callback))
    except CouldNotMinimize as e:
        # This is to double-check that there isn't a bug in
        # `pysource-minimize`/`pysource-codegen`.
        # `pysource-minimize` *should* never produce code that's invalid syntax.
        try:
            ast.parse(code)
        except SyntaxError:
            raise e from None
        else:
            maybe_bug = MinimizedSourceCode(code)

    return FuzzResult(seed, maybe_bug, args.executable, only_new_bugs=only_new_bugs)


def run_fuzzer_concurrently(args: ResolvedCliArgs) -> list[FuzzResult]:
    num_seeds = len(args.seeds)
    print(
        f"Concurrently running the fuzzer on "
        f"{num_seeds} randomly generated source-code "
        f"file{'s' if num_seeds != 1 else ''}..."
    )
    bugs: list[FuzzResult] = []
    with concurrent.futures.ProcessPoolExecutor() as executor:
        fuzz_result_futures = [
            executor.submit(fuzz_code, seed, args) for seed in args.seeds
        ]
        try:
            for i, future in enumerate(
                concurrent.futures.as_completed(fuzz_result_futures), start=1
            ):
                fuzz_result = future.result()
                if not args.quiet:
                    fuzz_result.print_description(i, num_seeds)
                if fuzz_result.maybe_bug:
                    bugs.append(fuzz_result)
        except KeyboardInterrupt:
            print("\nShutting down the ProcessPoolExecutor due to KeyboardInterrupt...")
            print("(This might take a few seconds)")
            executor.shutdown(cancel_futures=True)
            raise
    return bugs


def run_fuzzer_sequentially(args: ResolvedCliArgs) -> list[FuzzResult]:
    num_seeds = len(args.seeds)
    print(
        f"Sequentially running the fuzzer on "
        f"{num_seeds} randomly generated source-code "
        f"file{'s' if num_seeds != 1 else ''}..."
    )
    bugs: list[FuzzResult] = []
    for i, seed in enumerate(args.seeds, start=1):
        fuzz_result = fuzz_code(seed, args)
        if not args.quiet:
            fuzz_result.print_description(i, num_seeds)
        if fuzz_result.maybe_bug:
            bugs.append(fuzz_result)
    return bugs


def run_fuzzer(args: ResolvedCliArgs) -> ExitCode:
    if len(args.seeds) <= 5:
        bugs = run_fuzzer_sequentially(args)
    else:
        bugs = run_fuzzer_concurrently(args)
    noun_phrase = "New bugs" if args.baseline_executable_path is not None else "Bugs"
    if bugs:
        print(colored(f"{noun_phrase} found in the following seeds:", "red"))
        print(*sorted(bug.seed for bug in bugs))
        return ExitCode(1)
    else:
        print(colored(f"No {noun_phrase.lower()} found!", "green"))
        return ExitCode(0)


def absolute_path(p: str) -> Path:
    return Path(p).absolute()


def parse_seed_argument(arg: str) -> int | range:
    """Helper for argument parsing"""
    if "-" in arg:
        start, end = map(int, arg.split("-"))
        if end <= start:
            raise argparse.ArgumentTypeError(
                f"Error when parsing seed argument {arg!r}: "
                f"range end must be > range start"
            )
        seed_range = range(start, end + 1)
        range_too_long = (
            f"Error when parsing seed argument {arg!r}: "
            f"maximum allowed range length is 1_000_000_000"
        )
        try:
            if len(seed_range) > 1_000_000_000:
                raise argparse.ArgumentTypeError(range_too_long)
        except OverflowError:
            raise argparse.ArgumentTypeError(range_too_long) from None
        return range(int(start), int(end) + 1)
    return int(arg)


class Executable(enum.StrEnum):
    RUFF = "ruff"
    TY = "ty"


@dataclass(slots=True)
class ResolvedCliArgs:
    seeds: list[Seed]
    _: KW_ONLY
    executable: Executable
    test_executable_path: Path
    baseline_executable_path: Path | None
    quiet: bool


def parse_args() -> ResolvedCliArgs:
    """Parse command-line arguments"""
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=RawDescriptionRichHelpFormatter
    )
    parser.add_argument(
        "seeds",
        type=parse_seed_argument,
        nargs="+",
        help="Either a single seed, or an inclusive range of seeds in the format `0-5`",
    )
    parser.add_argument(
        "--only-new-bugs",
        action="store_true",
        help=(
            "Only report bugs if they exist on the current branch, "
            "but *didn't* exist on the released version "
            "installed into the Python environment we're running in"
        ),
    )
    parser.add_argument(
        "--quiet",
        action="store_true",
        help="Print fewer things to the terminal while running the fuzzer",
    )
    parser.add_argument(
        "--test-executable",
        help=(
            "Executable to test. "
            "Defaults to a fresh build of the currently checked-out branch."
        ),
        type=absolute_path,
    )
    parser.add_argument(
        "--baseline-executable",
        help=(
            "Executable to compare results against. "
            "Defaults to whatever version is installed "
            "in the Python environment."
        ),
        type=absolute_path,
    )
    parser.add_argument(
        "--bin",
        help="Which executable to test.",
        required=True,
        choices=[member.value for member in Executable],
    )

    args = parser.parse_args()

    executable = Executable(args.bin)

    if args.baseline_executable:
        if not args.only_new_bugs:
            parser.error(
                "Specifying `--baseline-executable` has no effect "
                "unless `--only-new-bugs` is also specified"
            )
        try:
            subprocess.run(
                [args.baseline_executable, "--version"], check=True, capture_output=True
            )
        except FileNotFoundError:
            parser.error(
                f"Bad argument passed to `--baseline-executable`: "
                f"no such file or executable {args.baseline_executable!r}"
            )
    elif args.only_new_bugs:
        try:
            version_proc = subprocess.run(
                [executable, "--version"], text=True, capture_output=True, check=True
            )
        except FileNotFoundError:
            parser.error(
                "`--only-new-bugs` was specified without specifying a baseline "
                f"executable, and no released version of `{executable}` appears to be "
                "installed in your Python environment"
            )
        else:
            if not args.quiet:
                version = version_proc.stdout.strip().split(" ")[1]
                print(
                    f"`--only-new-bugs` was specified without specifying a baseline "
                    f"executable; falling back to using `{executable}=={version}` as "
                    f"the baseline (the version of `{executable}` installed in your "
                    f"current Python environment)"
                )

    if not args.test_executable:
        print(
            "Running `cargo build --profile=profiling` since no test executable was specified...",
            flush=True,
        )
        cmd: list[str] = [
            "cargo",
            "build",
            "--profile",
            "profiling",
            "--locked",
            "--color",
            "always",
            "--bin",
            executable,
        ]
        try:
            subprocess.run(cmd, check=True, capture_output=True, text=True)
        except subprocess.CalledProcessError as e:
            print(e.stderr)
            raise
        args.test_executable = Path("target", "profiling", executable)
        assert args.test_executable.is_file()

    seed_arguments: list[range | int] = args.seeds
    seen_seeds: set[int] = set()
    for arg in seed_arguments:
        if isinstance(arg, int):
            seen_seeds.add(arg)
        else:
            seen_seeds.update(arg)

    return ResolvedCliArgs(
        sorted(map(Seed, seen_seeds)),
        quiet=args.quiet,
        executable=executable,
        test_executable_path=args.test_executable,
        baseline_executable_path=args.baseline_executable,
    )


def main() -> NoReturn:
    args = parse_args()
    raise SystemExit(run_fuzzer(args))


if __name__ == "__main__":
    main()
