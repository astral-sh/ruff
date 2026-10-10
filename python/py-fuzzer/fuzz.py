"""
Run a Ruff executable on randomly generated (but syntactically valid)
Python source-code files.

This script can be installed into a virtual environment using
`uv pip install -e ./python/py-fuzzer` from the Ruff repository root,
or can be run using `uv run --project=./python/py-fuzzer fuzz`
(in which case the virtual environment does not need to be activated).
Note that using `uv run --project` rather than `uvx --from` means that
uv will respect the script's lockfile.

When `--write-github-issue` writes a report, the report identifies the test
executable's build commit. Writing a report requires `GITHUB_SERVER_URL`,
`GITHUB_REPOSITORY`, and `GITHUB_RUN_ID` to be set. If any are missing, the
fuzzer exits with an error. These are the GitHub server URL, `OWNER/REPO`, and
run ID from the workflow URL; GitHub Actions sets them automatically.

On normal completion, the fuzzer exits with status 0 when it checks every seed
and finds no bugs, 1 when it checks every seed and finds bugs, and 2 when it
leaves seeds unchecked.

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
import contextlib
import enum
import json
import multiprocessing
import operator
import os
import re
import signal
import subprocess
import sys
import tempfile
import threading
import time
from collections.abc import Sequence
from dataclasses import KW_ONLY, dataclass
from pathlib import Path
from typing import Final, NamedTuple, NewType, NoReturn, assert_never

from jinja2 import Environment, FileSystemLoader, StrictUndefined
from pysource_codegen import generate as generate_random_code
from pysource_minimize import CouldNotMinimize, minimize as minimize_repro
from rich_argparse import RawDescriptionRichHelpFormatter
from termcolor import colored

Seed = NewType("Seed", int)
ExitCode = NewType("ExitCode", int)

TY_TARGET_PLATFORM: Final = "linux"

# Reserve time within the budget for the final summary and any issue report.
REPORTING_RESERVE_SECONDS: Final = 60

# Leave at least one minute for checking seeds and minimizing failures, in
# addition to the reserve.
MINIMUM_BUDGET_MINUTES: Final = REPORTING_RESERVE_SECONDS / 60 + 1

# GitHub issue descriptions are reportedly limited to 65,536 Unicode code points:
# https://github.com/dead-claudia/github-limits#issue-description
# Each UTF-8-encoded code point takes at least one byte, so this lower byte
# limit leaves headroom.
MAX_GITHUB_ISSUE_BODY_BYTES: Final = 60_000

# ty supports `--python-version=3.8`, but typeshed only supports 3.10+,
# so that's probably the oldest version we can usefully test with.
OLDEST_SUPPORTED_PYTHON: Final = "3.10"


class MinimizationTimedOut(Exception):
    """The fuzzing deadline was reached or the run was cancelled during minimization."""


def run_executable(command: Sequence[str | Path], *, input: str | None = None) -> int:
    """Run a command with a five-second timeout and return its exit code.

    Send `input` to standard input if provided; otherwise, inherit standard
    input from the caller. Standard output and standard error are discarded.
    On POSIX, start the command in a new session and, on timeout, kill its
    process group to terminate any children in that group without killing
    the caller. On other platforms, kill only the command. Then raise
    `subprocess.TimeoutExpired`.

    For other errors or interruptions while communicating with the command,
    terminate it if it is still running and wait for it to exit, then re-raise
    the exception.

    Using a process group here is superior due to the fact that ty can
    spawn `uv workspace metadata` in a subprocess when `TY_UV=1` is set;
    killing only ty could leave uv running.
    """
    with subprocess.Popen(
        command,
        stdin=subprocess.PIPE if input is not None else None,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        text=True,
        start_new_session=os.name == "posix",
    ) as process:
        try:
            process.communicate(input=input, timeout=5)
        except BaseException as error:
            # Ctrl+C can interrupt communicate() while the checker is still running;
            # on POSIX, the checker is in a separate session and doesn't receive the signal.
            # `poll()` returns `None` when the checker is still running, so we can kill it
            # before re-raising the interruption.
            # On POSIX, a timeout may leave children in the checker's process group
            # even after the checker exits, so we try to kill the group regardless.
            if isinstance(error, subprocess.TimeoutExpired) or process.poll() is None:
                if os.name == "posix":
                    # The process group may have disappeared
                    # if its members exited, leaving nothing in the group to kill.
                    # That would result in a `ProcessLookupError` that can be safely ignored.
                    with contextlib.suppress(ProcessLookupError):
                        os.killpg(process.pid, signal.SIGKILL)
                else:
                    # On Windows, `Popen.kill()` handles the race where the process has
                    # already exited, so the suppression above is unnecessary.
                    # https://github.com/python/cpython/blob/v3.12.0/Lib/subprocess.py#L1518-L1532
                    process.kill()
                process.wait()
            raise
        return process.wait()


def ty_contains_bug(code: str, *, ty_executable: Path) -> bool:
    """Return `True` if the code triggers a panic in type-checking code."""
    with tempfile.TemporaryDirectory() as tempdir:
        input_file = Path(tempdir, "input.py")
        input_file.write_text(code)
        command: list[str | Path] = [
            ty_executable,
            "check",
            input_file,
            "--python-version",
            OLDEST_SUPPORTED_PYTHON,
            "--python-platform",
            TY_TARGET_PLATFORM,
        ]
        try:
            returncode = run_executable(command)
        except subprocess.TimeoutExpired:
            return True
    return returncode not in {0, 1, 2}


def ruff_contains_bug(code: str, *, ruff_executable: Path) -> bool:
    """Return `True` if the code triggers a parser error."""
    command: list[str | Path] = [
        ruff_executable,
        "check",
        # Keep project settings out of parser checks, including for older Ruff versions.
        "--isolated",
        "--config",
        "lint.select=[]",
        "--no-cache",
        "--target-version",
        "py314",
        "--preview",
        "-",
    ]
    try:
        returncode = run_executable(command, input=code)
    except subprocess.TimeoutExpired:
        return True
    return returncode != 0


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


@dataclass(slots=True, kw_only=True, frozen=True)
class Bug:
    """A bug reproducer and whether minimization succeeded."""

    source: str
    minimization_succeeded: bool


@dataclass(slots=True, frozen=True)
class FuzzResult:
    # The seed used to generate the random Python file.
    # The same seed always generates the same file.
    seed: Seed
    # If we found a bug, this contains a reproducer. If not, it is `None`.
    maybe_bug: Bug | None
    # The executable we're testing
    executable: Executable
    _: KW_ONLY
    only_new_bugs: bool
    minimization_timed_out: bool

    def print_description(self, index: int, num_seeds: int) -> None:
        """Print the result of checking this seed."""
        progress = f"[{index}/{num_seeds}]"
        msg = (
            colored(f"Ran fuzzer on seed {self.seed}", "red")
            if self.maybe_bug is not None
            else colored(f"Ran fuzzer successfully on seed {self.seed}", "green")
        )
        print(f"{msg:<60} {progress:>15}", flush=True)

        new = "new " if self.only_new_bugs else ""

        if self.maybe_bug is not None:
            match self.executable:
                case Executable.RUFF:
                    panic_message = (
                        f"The following code triggers a {new}parser bug or timeout:"
                    )
                case Executable.TY:
                    panic_message = (
                        f"The following code triggers a {new}ty panic or timeout with "
                        f"`--python-version={OLDEST_SUPPORTED_PYTHON} --python-platform={TY_TARGET_PLATFORM}`:"
                    )
                case _ as unreachable:
                    assert_never(unreachable)

            if self.maybe_bug.minimization_succeeded:
                print(f"Minimized seed {self.seed}:")
            else:
                print(f"Original source for seed {self.seed}:")
            print(colored(panic_message, "red"))
            print()
            print(self.maybe_bug.source)
            print(flush=True)


def contains_reportable_bug(code: str, args: ResolvedCliArgs) -> bool:
    """Return whether the test executable finds a bug in the code.

    If a baseline executable is provided, only return `True` if the baseline
    does not find a bug in the code.
    """
    if args.baseline_executable_path is None:
        return contains_bug(
            code, executable=args.executable, executable_path=args.test_executable_path
        )
    return contains_new_bug(
        code,
        executable=args.executable,
        test_executable_path=args.test_executable_path,
        baseline_executable_path=args.baseline_executable_path,
    )


def fuzz_code(
    seed: Seed,
    args: ResolvedCliArgs,
    deadline: float | None,
    cancelled: threading.Event,
) -> FuzzResult | None:
    """Check one seed and minimize any failure, or return None if it was not checked."""
    if cancelled.is_set() or (deadline is not None and time.monotonic() >= deadline):
        return None

    code = generate_random_code(seed)

    if cancelled.is_set() or (deadline is not None and time.monotonic() >= deadline):
        return None

    if not contains_reportable_bug(code, args):
        return FuzzResult(
            seed,
            None,
            args.executable,
            only_new_bugs=args.baseline_executable_path is not None,
            minimization_timed_out=False,
        )

    maybe_bug = Bug(source=code, minimization_succeeded=False)
    minimization_timed_out = False

    if not args.quiet:
        print(f"Found a bug for seed {seed}; minimizing...", flush=True)

    def bounded_callback(candidate: str) -> bool:
        """Return whether a candidate triggers a bug.

        Raise `MinimizationTimedOut` if the deadline or cancellation is observed
        before or after checking the candidate.
        """
        if cancelled.is_set() or (
            deadline is not None and time.monotonic() >= deadline
        ):
            raise MinimizationTimedOut
        found = contains_reportable_bug(candidate, args)
        if cancelled.is_set() or (
            deadline is not None and time.monotonic() >= deadline
        ):
            raise MinimizationTimedOut
        return found

    try:
        minimized = minimize_repro(code, bounded_callback)
    except CouldNotMinimize as e:
        # This is to double-check that there isn't a bug in
        # `pysource-minimize`/`pysource-codegen`.
        # `pysource-minimize` *should* never produce code that's invalid syntax.
        try:
            ast.parse(code)
        except SyntaxError:
            raise e from None
        if not args.quiet:
            print(f"Could not minimize seed {seed}: {e}", file=sys.stderr, flush=True)
    except MinimizationTimedOut:
        minimization_timed_out = True
    # An input whose execution time is close to the timeout may time out during
    # the minimizer's initial check but finish when the same source is rechecked.
    # `pysource-minimize` currently raises a plain `ValueError` in this case, so
    # catch it to preserve the original finding. This also catches unrelated
    # `ValueError`s. Remove this handler once a release containing the fix in
    # https://github.com/15r10nk/pysource-minimize/pull/50 is the minimum
    # supported `pysource-minimize` version.
    except ValueError as e:
        if not args.quiet:
            print(f"Could not minimize seed {seed}: {e}", file=sys.stderr, flush=True)
    else:
        maybe_bug = Bug(source=minimized, minimization_succeeded=True)

    return FuzzResult(
        seed=seed,
        maybe_bug=maybe_bug,
        executable=args.executable,
        only_new_bugs=args.baseline_executable_path is not None,
        minimization_timed_out=minimization_timed_out,
    )


class FuzzRunResult(NamedTuple):
    """The bugs found and the seeds left unchecked in a fuzzing run."""

    bugs: list[FuzzResult]
    unchecked_seeds: list[Seed]


def run_fuzzer_concurrently(
    args: ResolvedCliArgs, deadline: float | None
) -> FuzzRunResult:
    """Check seeds concurrently and minimize failures, returning failures and unchecked seeds."""
    num_seeds = len(args.seeds)
    print(
        f"Concurrently running the fuzzer on "
        f"{num_seeds} randomly generated source-code "
        f"file{'s' if num_seeds != 1 else ''}..."
    )
    bugs: list[FuzzResult] = []
    unchecked: list[Seed] = []

    with multiprocessing.Manager() as manager:
        cancelled = manager.Event()
        with concurrent.futures.ProcessPoolExecutor() as executor:
            try:
                futures = {
                    executor.submit(fuzz_code, seed, args, deadline, cancelled): seed
                    for seed in args.seeds
                }
                for i, future in enumerate(
                    concurrent.futures.as_completed(futures), start=1
                ):
                    fuzz_result = future.result()
                    if fuzz_result is None:
                        unchecked.append(futures[future])
                        continue
                    if not args.quiet:
                        fuzz_result.print_description(i, num_seeds)
                    if fuzz_result.maybe_bug is not None:
                        bugs.append(fuzz_result)
            except BaseException:
                cancelled.set()
                executor.shutdown(cancel_futures=True)
                raise
    return FuzzRunResult(bugs=bugs, unchecked_seeds=unchecked)


def run_fuzzer_sequentially(
    args: ResolvedCliArgs, *, deadline: float | None
) -> FuzzRunResult:
    """Check seeds sequentially and minimize failures, returning failures and unchecked seeds."""
    num_seeds = len(args.seeds)
    print(
        f"Sequentially running the fuzzer on "
        f"{num_seeds} randomly generated source-code "
        f"file{'s' if num_seeds != 1 else ''}..."
    )
    bugs: list[FuzzResult] = []
    cancelled = threading.Event()

    for i, seed in enumerate(args.seeds, start=1):
        fuzz_result = fuzz_code(seed, args, deadline, cancelled)
        if fuzz_result is None:
            return FuzzRunResult(bugs=bugs, unchecked_seeds=args.seeds[i - 1 :])
        if not args.quiet:
            fuzz_result.print_description(i, num_seeds)
        if fuzz_result.maybe_bug is not None:
            bugs.append(fuzz_result)

    return FuzzRunResult(bugs=bugs, unchecked_seeds=[])


@dataclass(slots=True, kw_only=True, frozen=True)
class RenderableReproducer:
    """A bug and its seed, paired with a Markdown fence for its source."""

    seed: Seed
    bug: Bug
    fence: str


def render_issue_body(
    bugs: list[FuzzResult],
    unchecked_seeds: list[Seed],
    *,
    rerun_command: str,
    executable: Executable,
    executable_revision: str,
    run_url: str,
    fuzzer_revision: str,
) -> str:
    """Render a GitHub issue body containing reproducers and unchecked seeds.

    Omit reproducers or unchecked seeds when including them alongside omission
    notices would exceed the issue body size limit.
    """
    match executable:
        case Executable.RUFF:
            failure_description = (
                "Each snippet caused Ruff to exit with a nonzero status or time out "
                "after five seconds during fuzzing."
            )
            reproduction_command = (
                "target/debug/ruff check --isolated --config 'lint.select=[]' "
                "--no-cache --target-version py314 --preview - < repro.py"
            )
        case Executable.TY:
            failure_description = (
                "Each snippet caused ty to exit with a status other than 0, 1, or 2, "
                "or time out after five seconds during fuzzing."
            )
            reproduction_command = (
                "target/debug/ty check repro.py "
                f"--python-version={OLDEST_SUPPORTED_PYTHON} "
                f"--python-platform={TY_TARGET_PLATFORM}"
            )
        case _ as unreachable:
            assert_never(unreachable)

    environment = Environment(
        loader=FileSystemLoader(Path(__file__).parent),
        undefined=StrictUndefined,
        autoescape=False,
        trim_blocks=True,
        lstrip_blocks=True,
    )

    template = environment.get_template("daily_fuzz_issue.md.jinja")
    unchecked_seeds = sorted(unchecked_seeds)
    selected_seeds: list[Seed] = []

    def render(reproducers: list[RenderableReproducer], *, omitted: bool) -> str:
        """Render selected reproducers and unchecked seeds with any omission notices."""
        return template.render(
            run_url=run_url,
            fuzzer_revision=fuzzer_revision,
            executable=executable,
            executable_revision=executable_revision,
            failure_description=failure_description,
            reproduction_command=reproduction_command,
            reproducers=reproducers,
            omitted=omitted,
            unchecked_count=len(unchecked_seeds),
            unchecked_seeds=selected_seeds,
            seeds_omitted=len(selected_seeds) < len(unchecked_seeds),
            seed_rerun_command=rerun_command,
        ).removesuffix("\n")

    reproducers: list[RenderableReproducer] = []
    omitted = False

    # Keep the report simple by listing each seed separately, even when several
    # have the same reproducer. The workflow logs list all failing seeds if the
    # issue body fills up.
    for result in sorted(bugs, key=operator.attrgetter("seed")):
        assert result.maybe_bug is not None
        longest_fence = max(
            (len(run) for run in re.findall(r"`+", result.maybe_bug.source)), default=0
        )
        renderable = RenderableReproducer(
            seed=result.seed,
            bug=result.maybe_bug,
            fence="`" * max(3, longest_fence + 1),
        )
        candidate = render([*reproducers, renderable], omitted=True)
        if len(candidate.encode("utf-8")) <= MAX_GITHUB_ISSUE_BODY_BYTES:
            reproducers.append(renderable)
        else:
            omitted = True

    remaining_bytes = MAX_GITHUB_ISSUE_BODY_BYTES - len(
        render(reproducers, omitted=omitted).encode("utf-8")
    )

    for seed in unchecked_seeds:
        seed_bytes = len(f"- `{seed}`\n".encode())
        if seed_bytes > remaining_bytes:
            break
        selected_seeds.append(seed)
        remaining_bytes -= seed_bytes

    return render(reproducers, omitted=omitted)


def write_github_issue_body(
    run_result: FuzzRunResult, args: ResolvedCliArgs, path: Path, *, rerun_command: str
) -> None:
    """Write a GitHub issue body for the fuzzing run to a Markdown file."""
    try:
        run_url = (
            f"{os.environ['GITHUB_SERVER_URL']}/{os.environ['GITHUB_REPOSITORY']}"
            f"/actions/runs/{os.environ['GITHUB_RUN_ID']}"
        )
    except KeyError as error:
        raise RuntimeError(
            f"--write-github-issue requires the {error.args[0]} environment variable"
        ) from None

    version_output = subprocess.check_output(
        [args.test_executable_path, "version", "--output-format=json"], text=True
    )

    commit_info = json.loads(version_output)["commit_info"]
    if commit_info is None:
        raise RuntimeError(
            f"{args.test_executable_path} does not report its build commit"
        )

    commit_hash = commit_info["commit_hash"]
    assert isinstance(commit_hash, str)

    fuzzer_revision = subprocess.check_output(
        ["git", "-C", Path(__file__).parent, "rev-parse", "HEAD"], text=True
    )

    body = render_issue_body(
        run_result.bugs,
        run_result.unchecked_seeds,
        rerun_command=rerun_command,
        executable=args.executable,
        executable_revision=commit_hash,
        run_url=run_url,
        fuzzer_revision=fuzzer_revision.strip(),
    )

    path.write_text(body, encoding="utf-8")


def run_fuzzer(args: ResolvedCliArgs) -> ExitCode:
    deadline = (
        time.monotonic() + args.budget_minutes * 60 - REPORTING_RESERVE_SECONDS
        if args.budget_minutes is not None
        else None
    )

    if len(args.seeds) <= 5:
        run_result = run_fuzzer_sequentially(args, deadline=deadline)
    else:
        run_result = run_fuzzer_concurrently(args, deadline=deadline)

    bugs = run_result.bugs
    unchecked = run_result.unchecked_seeds
    unfinished = sorted(bug.seed for bug in bugs if bug.minimization_timed_out)

    if unfinished:
        print("Seeds not minimized before the shared deadline:", *unfinished)

    if unchecked:
        print(
            f"Budget expired before checking {len(unchecked)} seed(s):",
            *sorted(unchecked),
        )

    noun_phrase = "New bugs" if args.baseline_executable_path is not None else "Bugs"

    if bugs:
        print(colored(f"{noun_phrase} found in the following seeds:", "red"))
        print(*sorted(bug.seed for bug in bugs))
    elif unchecked:
        print(f"No {noun_phrase.lower()} found among the checked seeds.")
    else:
        print(colored(f"No {noun_phrase.lower()} found!", "green"))

    if bugs or unchecked:
        message = "To check all seeds that failed or were left unchecked from the repository root:"
        seeds = sorted({bug.seed for bug in bugs}.union(unchecked))
        rerun_command = (
            f"uv run --project=./python/py-fuzzer fuzz --bin={args.executable} "
            + " ".join(map(str, seeds))
        )
        print()
        print(colored(message, "cyan"))
        print(colored(rerun_command, "cyan"))
        if args.github_issue_path is not None:
            write_github_issue_body(
                run_result, args, args.github_issue_path, rerun_command=rerun_command
            )

    if unchecked:
        return ExitCode(2)

    return ExitCode(1 if bugs else 0)


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


@dataclass(slots=True, frozen=True, kw_only=True)
class ResolvedCliArgs:
    seeds: list[Seed]
    _: KW_ONLY
    executable: Executable
    test_executable_path: Path
    baseline_executable_path: Path | None
    quiet: bool
    github_issue_path: Path | None
    budget_minutes: float | None


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
        "--budget-minutes",
        type=float,
        metavar="MINUTES",
        help=(
            "Time budget after any automatic build for checking seeds, minimizing "
            "failures, and reporting results "
            f"(no limit by default; minimum: {MINIMUM_BUDGET_MINUTES:g} minutes). "
            "Reserves 60 seconds for reporting. Once that reserve is reached, "
            "stops starting new seed checks and stops minimization at the next "
            "candidate check. Work already in progress can finish later, so total "
            "runtime may exceed the budget."
        ),
    )
    parser.add_argument(
        "--write-github-issue",
        type=Path,
        metavar="PATH",
        help="Write a GitHub issue body to PATH if bugs are found or seeds are unchecked (intended for CI workflows)",
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

    if args.budget_minutes is not None and args.budget_minutes < MINIMUM_BUDGET_MINUTES:
        parser.error(f"--budget-minutes must be at least {MINIMUM_BUDGET_MINUTES:g}")

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
            args.baseline_executable = Path(executable)

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

    seen_seeds: set[int] = set()

    for arg in args.seeds:
        if isinstance(arg, int):
            seen_seeds.add(arg)
        else:
            seen_seeds.update(arg)

    return ResolvedCliArgs(
        seeds=sorted(map(Seed, seen_seeds)),
        quiet=args.quiet,
        executable=executable,
        test_executable_path=args.test_executable,
        baseline_executable_path=args.baseline_executable,
        github_issue_path=args.write_github_issue,
        budget_minutes=args.budget_minutes,
    )


def main() -> NoReturn:
    args = parse_args()
    raise SystemExit(run_fuzzer(args))


if __name__ == "__main__":
    main()
