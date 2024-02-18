import asyncio
import dataclasses
import json
from enum import Enum
from pathlib import Path
from typing import Awaitable, TypeVar

from ruff_ecosystem import logger
from ruff_ecosystem.check import compare_check, markdown_check_result
from ruff_ecosystem.format import (
    FormatComparison,
    compare_format,
    markdown_format_result,
)
from ruff_ecosystem.projects import (
    Project,
    RuffCommand,
)
from ruff_ecosystem.types import Comparison, Result, Serializable

T = TypeVar("T")
GITHUB_MAX_COMMENT_LENGTH = 65536


class OutputFormat(Enum):
    markdown = "markdown"
    json = "json"


async def main(
    command: RuffCommand,
    baseline_executable: Path,
    comparison_executable: Path,
    targets: list[Project],
    project_dir: Path,
    format: OutputFormat,
    format_comparison: FormatComparison | None,
    max_parallelism: int = 50,
    raise_on_failure: bool = False,
) -> None:
    logger.debug("Using command %s", command.value)
    logger.debug("Using baseline executable at %s", baseline_executable)
    logger.debug("Using comparison executable at %s", comparison_executable)
    logger.debug("Using checkout_dir directory %s", project_dir)
    if format_comparison:
        logger.debug("Using format comparison type %s", format_comparison.value)
    logger.debug("Checking %s targets", len(targets))

    # Limit parallelism to avoid high memory consumption
    semaphore = asyncio.Semaphore(max_parallelism)

    async def limited_parallelism(coroutine: Awaitable[T]) -> T:
        async with semaphore:
            return await coroutine

    comparisons: list[BaseException | Comparison] = await asyncio.gather(
        *[
            limited_parallelism(
                clone_and_compare(
                    command,
                    baseline_executable,
                    comparison_executable,
                    target,
                    project_dir,
                    format_comparison,
                )
            )
            for target in targets
        ],
        return_exceptions=not raise_on_failure,
    )
    comparisons_by_target = dict(zip(targets, comparisons, strict=True))

    # Split comparisons into errored / completed
    errored: list[tuple[Project, BaseException]] = []
    completed: list[tuple[Project, Comparison]] = []
    for target, comparison in comparisons_by_target.items():
        if isinstance(comparison, BaseException):
            errored.append((target, comparison))
        else:
            completed.append((target, comparison))

    result = Result(completed=completed, errored=errored)

    match format:
        case OutputFormat.json:
            print(json.dumps(result, indent=4, cls=JSONEncoder))
        case OutputFormat.markdown:
            match command:
                case RuffCommand.check:
                    print(markdown_check_result(result))
                case RuffCommand.format:
                    print(markdown_format_result(result))
                case _:
                    raise ValueError(f"Unknown target Ruff command {command}")
        case _:
            raise ValueError(f"Unknown output format {format}")

    return None


async def clone_and_compare(
    command: RuffCommand,
    baseline_executable: Path,
    comparison_executable: Path,
    target: Project,
    project_dir: Path,
    format_comparison: FormatComparison | None,
) -> Comparison:
    """Check a specific repository against two versions of ruff."""
    assert ":" not in target.repo.owner
    assert ":" not in target.repo.name

    match command:
        case RuffCommand.check:
            compare, options, overrides, kwargs = (
                compare_check,
                target.check_options,
                target.config_overrides,
                {},
            )
        case RuffCommand.format:
            compare, options, overrides, kwargs = (
                compare_format,
                target.format_options,
                target.config_overrides,
                {"format_comparison": format_comparison},
            )
        case _:
            raise ValueError(f"Unknown target Ruff command {command}")

    checkout_dir = project_dir.joinpath(f"{target.repo.owner}:{target.repo.name}")
    cloned_repo = await target.repo.clone(checkout_dir)

    try:
        return await compare(
            baseline_executable,
            comparison_executable,
            options,
            overrides,
            cloned_repo,
            **kwargs,
        )
    except ExceptionGroup as e:
        raise e.exceptions[0] from e


class JSONEncoder(json.JSONEncoder):
    def default(self, o: object):
        if isinstance(o, Serializable):
            return o.jsonable()
        if dataclasses.is_dataclass(o):
            return dataclasses.asdict(o)
        if isinstance(o, set):
            return tuple(o)
        if isinstance(o, Path):
            return str(o)
        if isinstance(o, Exception):
            return str(o)
        return super().default(o)
