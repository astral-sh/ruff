from __future__ import annotations

import abc
import dataclasses
import difflib
from dataclasses import dataclass, is_dataclass
from typing import TYPE_CHECKING, Any, Generator, Iterable, Sequence

if TYPE_CHECKING:
    from ruff_ecosystem.projects import ClonedRepository, Project


class Serializable(abc.ABC):
    """
    Allows serialization of content by casting to a JSON-compatible type.
    """

    def jsonable(self) -> Any:
        # Default implementation for dataclasses
        if is_dataclass(self):
            return dataclasses.asdict(self)

        raise NotImplementedError()


class Diff(Serializable):
    def __init__(self, lines: Iterable[str]) -> None:
        self.lines = list(lines)

        # Compute added and removed lines once
        self.added = list(line[2:] for line in self.lines if line.startswith("+ "))
        self.removed = list(line[2:] for line in self.lines if line.startswith("- "))

    def __bool__(self) -> bool:
        return bool(self.added or self.removed)

    def __iter__(self) -> Generator[str, None, None]:
        yield from self.lines

    @property
    def lines_added(self):
        return len(self.added)

    @property
    def lines_removed(self):
        return len(self.removed)

    @classmethod
    def new(cls, baseline: Sequence[str], comparison: Sequence[str]):
        return cls(difflib.ndiff(baseline, comparison))

    def jsonable(self) -> Any:
        return self.lines


@dataclass(frozen=True)
class Result(Serializable):
    errored: list[tuple[Project, Exception]]
    completed: list[tuple[Project, Comparison]]


@dataclass(frozen=True)
class Comparison(Serializable):
    diff: Diff
    repo: ClonedRepository


class RuffError(Exception):
    """An error reported by ruff."""
