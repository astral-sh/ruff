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

from ruff_ecosystem.defaults import DEFAULT_TARGETS
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

    with cache_context as cache:
        loop = asyncio.get_event_loop()
        main_task = asyncio.ensure_future(
            main(
                command=RuffCommand(args.ruff_command),
                ruff_baseline_executable=args.ruff_baseline,
                ruff_comparison_executable=args.ruff_comparison,
                targets=DEFAULT_TARGETS,
                format=OutputFormat(args.output_format),
                cache=Path(cache),
                raise_on_failure=args.pdb,
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
        choices=[option.name for option in OutputFormat],
        default="json",
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
        "ruff_command",
        choices=[option.name for option in RuffCommand],
        help="The Ruff command to test",
    )
    parser.add_argument(
        "ruff_baseline",
        type=Path,
    )
    parser.add_argument(
        "ruff_comparison",
        type=Path,
    )

    return parser.parse_args()
