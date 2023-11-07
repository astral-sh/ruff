import argparse
import asyncio
import logging
import os
import shutil
import sys
import sysconfig
import tempfile
from contextlib import nullcontext
from pathlib import Path
from signal import SIGINT, SIGTERM

from ruff_ecosystem import logger
from ruff_ecosystem.defaults import DEFAULT_TARGETS
from ruff_ecosystem.format import FormatComparison
from ruff_ecosystem.main import OutputFormat, main
from ruff_ecosystem.projects import RuffCommand


def excepthook(type, value, tb):
    if hasattr(sys, "ps1") or not sys.stderr.isatty():
        # we are in interactive mode or we don't have a tty so call the default
        sys.__excepthook__(type, value, tb)
    else:
        import pdb
        import traceback

        traceback.print_exception(type, value, tb)
        print()
        pdb.post_mortem(tb)


def entrypoint():
    args = parse_args()

    if args.pdb:
        sys.excepthook = excepthook

    if args.verbose:
        logging.basicConfig(level=logging.DEBUG)
    else:
        logging.basicConfig(level=logging.INFO)

    # Use a temporary directory for caching if no cache is specified
    cache_context = (
        tempfile.TemporaryDirectory() if not args.cache else nullcontext(args.cache)
    )

    baseline_executable = args.baseline_executable
    if not args.baseline_executable.exists():
        baseline_executable = get_executable_path(str(args.baseline_executable))
        if not baseline_executable:
            print(
                f"Could not find ruff baseline executable: {args.baseline_executable}",
                sys.stderr,
            )
            exit(1)
        logger.info(
            "Resolved baseline executable %s to %s",
            args.baseline_executable,
            baseline_executable,
        )

    comparison_executable = args.comparison_executable
    if not args.comparison_executable.exists():
        comparison_executable = get_executable_path(str(args.comparison_executable))
        if not comparison_executable:
            print(
                f"Could not find ruff comparison executable: {args.comparison_executable}",
                sys.stderr,
            )
            exit(1)
        logger.info(
            "Resolved comparison executable %s to %s",
            args.comparison_executable,
            comparison_executable,
        )

    targets = DEFAULT_TARGETS
    if args.force_preview:
        targets = [target.with_preview_enabled() for target in targets]

    format_comparison = (
        FormatComparison(args.format_comparison)
        if args.ruff_command == RuffCommand.format.value
        else None
    )

    with cache_context as cache:
        loop = asyncio.get_event_loop()
        main_task = asyncio.ensure_future(
            main(
                command=RuffCommand(args.ruff_command),
                baseline_executable=baseline_executable,
                comparison_executable=comparison_executable,
                targets=targets,
                format=OutputFormat(args.output_format),
                project_dir=Path(cache),
                raise_on_failure=args.pdb,
                format_comparison=format_comparison,
            )
        )
        # https://stackoverflow.com/a/58840987/3549270
        for signal in [SIGINT, SIGTERM]:
            loop.add_signal_handler(signal, main_task.cancel)
        try:
            loop.run_until_complete(main_task)
        finally:
            loop.close()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Check two versions of ruff against a corpus of open-source code.",
    )

    # TODO: Support non-default `--targets`
    # parser.add_argument(
    #     "--targets",
    #     type=Path,
    #     help=(
    #         "Optional JSON files to use over the default repositories. "
    #         "Supports both github_search_*.jsonl and known-github-tomls.jsonl."
    #     ),
    # )
    parser.add_argument(
        "--cache",
        type=Path,
        help="Location for caching cloned repositories",
    )
    parser.add_argument(
        "--output-format",
        choices=[option.value for option in OutputFormat],
        default="markdown",
        help="Location for caching cloned repositories",
    )
    parser.add_argument(
        "-v",
        "--verbose",
        action="store_true",
        help="Enable debug logging",
    )
    parser.add_argument(
        "--pdb",
        action="store_true",
        help="Enable debugging on failure",
    )
    parser.add_argument(
        "--force-preview",
        action="store_true",
        help="Force preview mode to be enabled for all projects",
    )
    parser.add_argument(
        "--format-comparison",
        choices=[option.value for option in FormatComparison],
        default=FormatComparison.ruff_and_ruff,
        help="Type of comparison to make when checking formatting.",
    )
    parser.add_argument(
        "ruff_command",
        choices=[option.value for option in RuffCommand],
        help="The Ruff command to test",
    )
    parser.add_argument(
        "baseline_executable",
        type=Path,
    )
    parser.add_argument(
        "comparison_executable",
        type=Path,
    )

    return parser.parse_args()


def get_executable_path(name: str) -> Path | None:
    # Add suffix for Windows executables
    name += ".exe" if sys.platform == "win32" and not name.endswith(".exe") else ""

    path = os.path.join(sysconfig.get_path("scripts"), name)

    # The executable in the current interpreter's scripts directory.
    if os.path.exists(path):
        return Path(path)

    # The executable in the global environment.
    environment_path = shutil.which(name)
    if environment_path:
        return Path(environment_path)

    return None
