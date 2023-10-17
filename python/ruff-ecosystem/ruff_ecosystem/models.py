from enum import Enum
from dataclasses import dataclass, field
from typing import Self, Iterator
import heapq
from pathlib import Path


class RuffCommand(Enum):
    check = "check"
    format = "format"


@dataclass(frozen=True)
class Repository:
    """
    A remote GitHub repository
    """

    owner: str
    name: str
    branch: str | None

    @property
    def fullname(self) -> str:
        return f"{self.owner}/{self.name}"

    @property
    def url(self: Self) -> str:
        return f"https://github.com/{self.owner}/{self.name}"


@dataclass(frozen=True)
class ClonedRepository(Repository):
    """
    A cloned GitHub repository, which includes the hash of the cloned commit.
    """

    commit_hash: str
    path: Path

    def url_for(self: Self, path: str, line_number: int | None = None) -> str:
        """
        Return the remote GitHub URL for the given path in this repository.
        """
        # Default to main branch
        url = f"https://github.com/{self.owner}/{self.name}/blob/{self.commit_hash}/{path}"
        if line_number:
            url += f"#L{line_number}"
        return url

    @property
    def url(self: Self) -> str:
        return f"https://github.com/{self.owner}/{self.name}@{self.commit_hash}"


@dataclass(frozen=True)
class Diff:
    """A diff between two runs of ruff."""

    removed: set[str]
    added: set[str]

    def __bool__(self: Self) -> bool:
        """Return true if this diff is non-empty."""
        return bool(self.removed or self.added)

    def lines(self: Self) -> Iterator[str]:
        """Iterate through the changed lines in diff format."""
        for line in heapq.merge(sorted(self.removed), sorted(self.added)):
            if line in self.removed:
                yield f"- {line}"
            else:
                yield f"+ {line}"


@dataclass(frozen=True)
class RuleChanges:
    changes: dict[str, tuple[int, int]] = field(default_factory=dict)

    def rule_codes(self) -> list[str]:
        return list(self.changes.keys())

    def items(self) -> Iterator[tuple[str, tuple[int, int]]]:
        return self.changes.items()

    def __setitem__(self, key: str, value: tuple[int, int]) -> None:
        self.changes[key] = value

    def __getitem__(self, key: str) -> tuple[int, int]:
        return self.changes.get(key, (0, 0))

    def __add__(self, other: Self) -> Self:
        if not isinstance(other, type(self)):
            return NotImplemented

        result = self.changes.copy()
        for rule_code, (added, removed) in other.changes.items():
            if rule_code in result:
                result[rule_code] = (
                    result[rule_code][0] + added,
                    result[rule_code][1] + removed,
                )
            else:
                result[rule_code] = (added, removed)

        return RuleChanges(changes=result)


@dataclass(frozen=True)
class CheckComparison:
    diff: Diff
    repo: ClonedRepository
    rule_changes: RuleChanges


@dataclass(frozen=True)
class CheckOptions:
    """
    Ruff check options
    """

    select: str = ""
    ignore: str = ""
    exclude: str = ""

    # Generating fixes is slow and verbose
    show_fixes: bool = False

    def summary(self) -> str:
        return f"select {self.select} ignore {self.ignore} exclude {self.exclude}"


@dataclass(frozen=True)
class FormatOptions:
    """
    Ruff format options
    """

    pass


@dataclass(frozen=True)
class Target:
    """
    An ecosystem target
    """

    repo: Repository
    check_options: CheckOptions = field(default_factory=CheckOptions)
    format_options: FormatOptions = field(default_factory=FormatOptions)


@dataclass(frozen=True)
class Result:
    total_added: int
    total_removed: int
    total_rule_changes: RuleChanges

    comparisons: tuple[Target, CheckComparison]
    errors: tuple[Target, Exception]
