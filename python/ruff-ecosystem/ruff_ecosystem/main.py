from ruff_ecosystem.models import (
    RuffCommand,
    Target,
    Diff,
    ClonedRepository,
    RuleChanges,
    CheckComparison,
    Result,
)
from pathlib import Path
from ruff_ecosystem import logger
import asyncio
from ruff_ecosystem.git import clone
from ruff_ecosystem.ruff import ruff_check, ruff_format
from ruff_ecosystem.emitters import Emitter
import difflib
from typing import TypeVar
import re

T = TypeVar("T")


async def main(
    command: RuffCommand,
    ruff_baseline_executable: Path,
    ruff_comparison_executable: Path,
    targets: list[Target],
    cache: Path | None,
    emitter: Emitter,
    max_parallelism: int = 50,
    raise_on_failure: bool = False,
) -> None:
    logger.debug("Using command %s", command.value)
    logger.debug("Using baseline executable at %s", ruff_baseline_executable)
    logger.debug("Using comparison executable at %s", ruff_comparison_executable)
    logger.debug("Using cache directory %s", cache)
    logger.debug("Checking %s targets", len(targets))

    semaphore = asyncio.Semaphore(max_parallelism)

    async def limited_parallelism(coroutine: T) -> T:
        async with semaphore:
            return await coroutine

    comparisons: list[Exception | CheckComparison] = await asyncio.gather(
        *[
            limited_parallelism(
                clone_and_compare(
                    command,
                    ruff_baseline_executable,
                    ruff_comparison_executable,
                    target,
                    cache,
                )
            )
            for target in targets
        ],
        return_exceptions=not raise_on_failure,
    )
    comparisons_by_target = dict(zip(targets, comparisons, strict=True))

    # Calculate totals
    total_removed = total_added = errors = 0
    total_rule_changes = RuleChanges()
    for comparison in comparisons_by_target.values():
        if isinstance(comparison, Exception):
            errors += 1
        else:
            total_removed += len(comparison.diff.removed)
            total_added += len(comparison.diff.added)
            total_rule_changes += comparison.rule_changes

    errors = []
    comparisons = []
    for target, comparison in comparisons_by_target.items():
        if isinstance(comparison, Exception):
            errors.append((target, comparison))
            continue

        if comparison.diff:
            comparisons.append((target, comparison))

        else:
            continue

    result = Result(
        total_added=total_added,
        total_removed=total_removed,
        total_rule_changes=total_rule_changes,
        comparisons=comparisons,
        errors=errors,
    )

    emitter.emit_result(result)
    return

    if total_removed == 0 and total_added == 0 and errors == 0:
        print("\u2705 ecosystem check detected no changes.")
        return

    s = "s" if errors != 1 else ""
    changes = f"(+{total_added}, -{total_removed}, {errors} error{s})"

    print(f"\u2139\ufe0f ecosystem check **detected changes**. {changes}")
    print()

    for target, comparison in comparisons_by_target.items():
        if isinstance(comparison, Exception):
            emitter.emit_error(target, comparison)
            continue

        if comparison.diff:
            emitter.emit_diff(target, comparison.diff, comparison.repo)

        else:
            continue

    if len(total_rule_changes.rule_codes()) > 0:
        print(f"Rules changed: {len(total_rule_changes.rule_codes())}")
        print()
        print("| Rule | Changes | Additions | Removals |")
        print("| ---- | ------- | --------- | -------- |")
        for rule, (additions, removals) in sorted(
            total_rule_changes.items(),
            key=lambda x: (x[1][0] + x[1][1]),
            reverse=True,
        ):
            print(f"| {rule} | {additions + removals} | {additions} | {removals} |")


async def clone_and_compare(
    command: RuffCommand,
    ruff_baseline_executable: Path,
    ruff_comparison_executable: Path,
    target: Target,
    cache: Path,
) -> CheckComparison:
    """Check a specific repository against two versions of ruff."""
    assert ":" not in target.repo.owner
    assert ":" not in target.repo.name

    match command:
        case RuffCommand.check:
            ruff_task, create_comparison, options = (
                ruff_check,
                create_check_comparison,
                target.check_options,
            )
        case RuffCommand.format:
            ruff_task, create_comparison, options = (
                ruff_format,
                create_format_comparison,
                target.format_options,
            )
        case _:
            raise ValueError(f"Unknowm target Ruff command {command}")

    checkout_dir = cache.joinpath(f"{target.repo.owner}:{target.repo.name}")
    async with clone(target.repo, checkout_dir) as cloned_repo:
        try:
            async with asyncio.TaskGroup() as tg:
                baseline_task = tg.create_task(
                    ruff_task(
                        executable=ruff_baseline_executable.resolve(),
                        path=cloned_repo.path,
                        name=cloned_repo.fullname,
                        options=options,
                    ),
                )
                comparison_task = tg.create_task(
                    ruff_task(
                        executable=ruff_comparison_executable.resolve(),
                        path=cloned_repo.path,
                        name=cloned_repo.fullname,
                        options=options,
                    ),
                )
        except ExceptionGroup as e:
            raise e.exceptions[0] from e

    return create_comparison(
        cloned_repo, baseline_task.result(), comparison_task.result()
    )


def create_check_comparison(
    repo: ClonedRepository, baseline_output: str, comparison_output: str
) -> CheckComparison:
    removed, added = set(), set()

    for line in difflib.ndiff(baseline_output, comparison_output):
        if line.startswith("- "):
            removed.add(line[2:])
        elif line.startswith("+ "):
            added.add(line[2:])

    diff = Diff(removed=removed, added=added)

    return CheckComparison(
        diff=diff, repo=repo, rule_changes=rule_changes_from_diff(diff)
    )


def rule_changes_from_diff(diff: Diff) -> RuleChanges:
    """
    Parse a diff from `ruff check` to determine the additions and removals for each rule.
    """
    rule_changes = RuleChanges()

    # Count rule changes
    for line in diff.lines():
        # Find rule change for current line or construction
        # + <rule>/<path>:<line>:<column>: <rule_code> <message>
        matches = re.search(r": ([A-Z]{1,4}[0-9]{3,4})", line)

        if matches is None:
            # Handle case where there are no regex matches e.g.
            # +                 "?application=AIRFLOW&authenticator=TEST_AUTH&role=TEST_ROLE&warehouse=TEST_WAREHOUSE" # noqa: E501, ERA001
            # Which was found in local testing
            continue

        rule_code = matches.group(1)

        # Get current additions and removals for this rule
        current_changes = rule_changes[rule_code]

        # Check if addition or removal depending on the first character
        if line[0] == "+":
            current_changes = (current_changes[0] + 1, current_changes[1])
        elif line[0] == "-":
            current_changes = (current_changes[0], current_changes[1] + 1)

        rule_changes[rule_code] = current_changes

    return rule_changes
