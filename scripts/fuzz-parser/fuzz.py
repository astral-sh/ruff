"""
Run the parser on randomly generated (but syntactically valid) Python source-code files.

To install all dependencies for this script into an environment using `uv`, run:
    uv pip install -r scripts/fuzz-parser/requirements.txt

Example invocations of the script:
- Run the fuzzer using seeds 0, 1, 2, 78 and 93 to generate the code:
  `python scripts/fuzz-parser/fuzz.py 0-2 78 93`
- Run the fuzzer concurrently using seeds in range 0-10 inclusive,
  but only reporting bugs that are new on your branch:
  `python scripts/fuzz-parser/fuzz.py 0-10 --new-bugs-only`
- Run the fuzzer concurrently on 10,000 different Python source-code files,
  using a random selection of seeds, and only print a summary at the end
  (the `shuf` command is Unix-specific):
  `python scripts/fuzz-parser/fuzz.py $(shuf -i 0-1000000 -n 10000) --quiet
"""

from __future__ import annotations

import argparse
import concurrent.futures
import os.path
import subprocess
from dataclasses import KW_ONLY, dataclass
from functools import partial
from typing import NewType

from pysource_codegen import generate as generate_random_code
from pysource_minimize import minimize as minimize_repro
from rich_argparse import RawDescriptionRichHelpFormatter
from termcolor import colored

MinimizedSourceCode = NewType("MinimizedSourceCode", str)
Seed = NewType("Seed", int)
ExitCode = NewType("ExitCode", int)


def contains_bug(code: str, *, ruff_executable: str) -> bool:
    """Return `True` if the code triggers a parser error."""
    completed_process = subprocess.run(
        [ruff_executable, "check", "--config", "lint.select=[]", "--no-cache", "-"],
        capture_output=True,
        text=True,
        input=code,
    )
    return completed_process.returncode != 0


def contains_new_bug(
    code: str, *, test_executable: str, baseline_executable: str
) -> bool:
    """Return `True` if the code triggers a *new* parser error.

    A "new" parser error is one that exists with `test_executable`,
    but did not exist with `baseline_executable`.
    """
    return contains_bug(code, ruff_executable=test_executable) and not contains_bug(
        code, ruff_executable=baseline_executable
    )


@dataclass(slots=True)
class FuzzResult:
    # The seed used to generate the random Python file.
    # The same seed always generates the same file.
    seed: Seed
    # If we found a bug, this will be the minimum Python code
    # required to trigger the bug. If not, it will be `None`.
    maybe_bug: MinimizedSourceCode | None

    def print_description(self, index: int, num_seeds: int) -> None:
        """Describe the results of fuzzing the parser with this seed."""
        progress = f"[{index}/{num_seeds}]"
        msg = (
            colored(f"Ran fuzzer on seed {self.seed}", "red")
            if self.maybe_bug
            else colored(f"Ran fuzzer successfully on seed {self.seed}", "green")
        )
        print(f"{msg:<60} {progress:>15}", flush=True)
        if self.maybe_bug:
            print(colored("The following code triggers a bug:", "red"))
            print()
            print(self.maybe_bug)
            print(flush=True)


def fuzz_code(
    seed: Seed,
    *,
    test_executable: str,
    baseline_executable: str,
    only_new_bugs: bool,
) -> FuzzResult:
    """Return a `FuzzResult` instance describing the fuzzing result from this seed."""
    code = generate_random_code(seed)
    has_bug = (
        contains_new_bug(
            code,
            test_executable=test_executable,
            baseline_executable=baseline_executable,
        )
        if only_new_bugs
        else contains_bug(code, ruff_executable=test_executable)
    )
    if has_bug:
        maybe_bug = MinimizedSourceCode(
            minimize_repro(code, partial(contains_bug, ruff_executable=test_executable))
        )
    else:
        maybe_bug = None
    return FuzzResult(seed, maybe_bug)


def run_fuzzer_concurrently(args: ResolvedCliArgs) -> list[FuzzResult]:
    num_seeds = len(args.seeds)
    print(
        f"Concurrently running the fuzzer on "
        f"{num_seeds} randomly generated source-code files..."
    )
    bugs: list[FuzzResult] = []
    with concurrent.futures.ProcessPoolExecutor() as executor:
        fuzz_result_futures = [
            executor.submit(
                fuzz_code,
                seed,
                test_executable=args.test_executable,
                baseline_executable=args.baseline_executable,
                only_new_bugs=args.only_new_bugs,
            )
            for seed in args.seeds
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
        f"{num_seeds} randomly generated source-code files..."
    )
    bugs: list[FuzzResult] = []
    for i, seed in enumerate(args.seeds, start=1):
        fuzz_result = fuzz_code(
            seed,
            test_executable=args.test_executable,
            baseline_executable=args.baseline_executable,
            only_new_bugs=args.only_new_bugs,
        )
        if not args.quiet:
            fuzz_result.print_description(i, num_seeds)
        if fuzz_result.maybe_bug:
            bugs.append(fuzz_result)
    return bugs


def main(args: ResolvedCliArgs) -> ExitCode:
    if len(args.seeds) <= 5:
        bugs = run_fuzzer_sequentially(args)
    else:
        bugs = run_fuzzer_concurrently(args)
    noun_phrase = "New bugs" if args.only_new_bugs else "Bugs"
    if bugs:
        print(colored(f"{noun_phrase} found in the following seeds:", "red"))
        print(*sorted(bug.seed for bug in bugs))
        return ExitCode(1)
    else:
        print(colored(f"No {noun_phrase.lower()} found!", "green"))
        return ExitCode(0)


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


@dataclass(slots=True)
class ResolvedCliArgs:
    seeds: list[Seed]
    _: KW_ONLY
    test_executable: str
    baseline_executable: str
    only_new_bugs: bool
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
            "but *didn't* exist on the released version of Ruff "
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
            "`ruff` executable to test. "
            "Defaults to a fresh build of the currently checked-out branch."
        ),
    )
    parser.add_argument(
        "--baseline-executable",
        help=(
            "`ruff` executable to compare results against. "
            "Defaults to whatever `ruff` version is installed "
            "in the Python environment."
        ),
    )

    args = parser.parse_args()

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
            ruff_version_proc = subprocess.run(
                ["ruff", "--version"], text=True, capture_output=True, check=True
            )
        except FileNotFoundError:
            parser.error(
                "`--only-new-bugs` was specified without specifying a baseline "
                "executable, and no released version of Ruff appears to be installed "
                "in your Python environment"
            )
        else:
            if not args.quiet:
                ruff_version = ruff_version_proc.stdout.strip().split(" ")[1]
                print(
                    f"`--only-new-bugs` was specified without specifying a baseline "
                    f"executable; falling back to using `ruff=={ruff_version}` as the "
                    f"baseline (the version of Ruff installed in your current Python "
                    f"environment)"
                )
        args.baseline_executable = "ruff"

    if not args.test_executable:
        print(
            "Running `cargo build --release` since no test executable was specified...",
            flush=True,
        )
        try:
            subprocess.run(
                ["cargo", "build", "--release", "--locked", "--color", "always"],
                check=True,
                capture_output=True,
                text=True,
            )
        except subprocess.CalledProcessError as e:
            print(e.stderr)
            raise
        args.test_executable = os.path.join("target", "release", "ruff")
        assert os.path.exists(args.test_executable)

    seed_arguments: list[range | int] = args.seeds
    seen_seeds: set[int] = set()
    for arg in seed_arguments:
        if isinstance(arg, int):
            seen_seeds.add(arg)
        else:
            seen_seeds.update(arg)

    return ResolvedCliArgs(
        sorted(map(Seed, seen_seeds)),
        only_new_bugs=args.only_new_bugs,
        quiet=args.quiet,
        test_executable=args.test_executable,
        baseline_executable=args.baseline_executable,
    )


if __name__ == "__main__":
    args = parse_args()
    raise SystemExit(main(args))
