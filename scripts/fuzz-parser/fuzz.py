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
  and only print a summary at the end:
  `python scripts/fuzz-parser/fuzz.py 1-10000 --quiet

N.B. The script takes a few seconds to get started, as the script needs to compile
your checked out version of ruff with `--release` as a first step before it
can actually start fuzzing.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import subprocess
from dataclasses import KW_ONLY, dataclass
from typing import NewType

from pysource_codegen import generate as generate_random_code
from pysource_minimize import minimize as minimize_repro
from termcolor import colored

MinimizedSourceCode = NewType("MinimizedSourceCode", str)
Seed = NewType("Seed", int)


def run_ruff(executable_args: list[str], code: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [*executable_args, "check", "--select=E999", "--no-cache", "-"],
        capture_output=True,
        text=True,
        input=code,
    )


def contains_bug(code: str, *, only_new_bugs: bool = False) -> bool:
    """Return True if the code triggers a parser error and False otherwise.

    If `only_new_bugs` is set to `True`,
    the function also runs an installed version of Ruff on the same source code,
    and only returns `True` if the bug appears on the branch you have currently
    checked out but *not* in the latest release.
    """
    new_result = run_ruff(["cargo", "run", "--release", "--"], code)
    if not only_new_bugs:
        return new_result.returncode != 0
    if new_result.returncode == 0:
        return False
    old_result = run_ruff(["ruff"], code)
    return old_result.returncode == 0


@dataclass(slots=True)
class FuzzResult:
    # The seed used to generate the random Python file.
    # The same seed always generates the same file.
    seed: Seed
    # If we found a bug, this will be the minimum Python code
    # required to trigger the bug. If not, it will be `None`.
    maybe_bug: MinimizedSourceCode | None

    def print_description(self) -> None:
        """Describe the results of fuzzing the parser with this seed."""
        if self.maybe_bug:
            print(colored(f"Ran fuzzer on seed {self.seed}", "red"))
            print(colored("The following code triggers a bug:", "red"))
            print()
            print(self.maybe_bug)
            print()
        else:
            print(colored(f"Ran fuzzer successfully on seed {self.seed}", "green"))


def fuzz_code(seed: Seed, only_new_bugs: bool) -> FuzzResult:
    """Return a `FuzzResult` instance describing the fuzzing result from this seed."""
    code = generate_random_code(seed)
    if contains_bug(code, only_new_bugs=only_new_bugs):
        try:
            new_code = minimize_repro(code, contains_bug)
        except ValueError:
            # `pysource_minimize.minimize()` sometimes raises `ValueError` internally.
            # Just ignore it if so, and use the original generated code;
            # minimizing the repro is a nice-to-have, but isn't crucial.
            new_code = code
        return FuzzResult(seed, MinimizedSourceCode(new_code))
    return FuzzResult(seed, None)


def run_fuzzer_concurrently(args: ResolvedCliArgs) -> list[FuzzResult]:
    print(
        f"Concurrently running the fuzzer on "
        f"{len(args.seeds)} randomly generated source-code files..."
    )
    bugs: list[FuzzResult] = []
    with concurrent.futures.ProcessPoolExecutor() as executor:
        fuzz_result_futures = [
            executor.submit(fuzz_code, seed, args.only_new_bugs) for seed in args.seeds
        ]
        try:
            for future in concurrent.futures.as_completed(fuzz_result_futures):
                fuzz_result = future.result()
                if not args.quiet:
                    fuzz_result.print_description()
                if fuzz_result.maybe_bug:
                    bugs.append(fuzz_result)
        except KeyboardInterrupt:
            print("\nShutting down the ProcessPoolExecutor due to KeyboardInterrupt...")
            print("(This might take a few seconds)")
            executor.shutdown(cancel_futures=True)
            raise
    return bugs


def run_fuzzer_sequentially(args: ResolvedCliArgs) -> list[FuzzResult]:
    print(
        f"Sequentially running the fuzzer on "
        f"{len(args.seeds)} randomly generated source-code files..."
    )
    bugs: list[FuzzResult] = []
    for seed in args.seeds:
        fuzz_result = fuzz_code(seed, only_new_bugs=args.only_new_bugs)
        if not args.quiet:
            fuzz_result.print_description()
        if fuzz_result.maybe_bug:
            bugs.append(fuzz_result)
    return bugs


def main(args: ResolvedCliArgs) -> None:
    if args.only_new_bugs:
        ruff_version = (
            subprocess.run(
                ["ruff", "--version"], text=True, capture_output=True, check=True
            )
            .stdout.strip()
            .split(" ")[1]
        )
        print(
            f"As you have selected `--only-new-bugs`, "
            f"bugs will only be reported if they appear on your current branch "
            f"but do *not* appear in `ruff=={ruff_version}`"
        )
    if len(args.seeds) <= 5:
        bugs = run_fuzzer_sequentially(args)
    else:
        bugs = run_fuzzer_concurrently(args)
    noun_phrase = "New bugs" if args.only_new_bugs else "Bugs"
    if bugs:
        print(colored(f"{noun_phrase} found in the following seeds:", "red"))
        print(*sorted(bug.seed for bug in bugs))
    else:
        print(colored(f"No {noun_phrase.lower()} found!", "green"))


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
    only_new_bugs: bool
    quiet: bool


def parse_args() -> ResolvedCliArgs:
    """Parse command-line arguments"""
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawTextHelpFormatter
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
    args = parser.parse_args()
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
    )


if __name__ == "__main__":
    args = parse_args()
    main(args)
