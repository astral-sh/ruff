"""
Abstractions and utilities for working with projects to run ecosystem checks on.
"""

from __future__ import annotations

import abc
import dataclasses
from asyncio import create_subprocess_exec
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from subprocess import DEVNULL, PIPE
from typing import Self

from ruff_ecosystem import logger
from ruff_ecosystem.types import Serializable


@dataclass(frozen=True)
class Project(Serializable):
    """
    An ecosystem target
    """

    repo: Repository
    check_options: CheckOptions = field(default_factory=lambda: CheckOptions())
    format_options: FormatOptions = field(default_factory=lambda: FormatOptions())

    def with_preview_enabled(self: Self) -> Self:
        return type(self)(
            repo=self.repo,
            check_options=self.check_options.with_options(preview=True),
            format_options=self.format_options.with_options(preview=True),
        )


class RuffCommand(Enum):
    check = "check"
    format = "format"


@dataclass(frozen=True)
class CommandOptions(Serializable, abc.ABC):
    def with_options(self: Self, **kwargs) -> Self:
        """
        Return a copy of self with the given options set.
        """
        return type(self)(**{**dataclasses.asdict(self), **kwargs})

    @abc.abstractmethod
    def to_ruff_args(self) -> list[str]:
        pass


@dataclass(frozen=True)
class CheckOptions(CommandOptions):
    """
    Ruff check options
    """

    select: str = ""
    ignore: str = ""
    exclude: str = ""
    preview: bool = False

    # Generating fixes is slow and verbose
    show_fixes: bool = False

    # Limit the number of reported lines per rule
    max_lines_per_rule: int | None = 50

    def to_ruff_args(self) -> list[str]:
        args = [
            "check",
            "--no-cache",
            "--exit-zero",
            f"--{'' if self.preview else 'no-'}preview",
        ]
        if self.select:
            args.extend(["--select", self.select])
        if self.ignore:
            args.extend(["--ignore", self.ignore])
        if self.exclude:
            args.extend(["--exclude", self.exclude])
        if self.show_fixes:
            args.extend(["--show-fixes", "--ecosystem-ci"])
        return args


@dataclass(frozen=True)
class FormatOptions(CommandOptions):
    """
    Format ecosystem check options.
    """

    preview: bool = False
    exclude: str = ""

    def to_ruff_args(self) -> list[str]:
        args = ["format", f"--{'' if self.preview else 'no-'}preview"]
        if self.exclude:
            args.extend(["--exclude", self.exclude])
        return args

    def to_black_args(self) -> list[str]:
        args: list[str] = []
        if self.exclude:
            args.extend(["--exclude", self.exclude])
        if self.preview:
            args.append("--preview")
        return args


class ProjectSetupError(Exception):
    """An error setting up a project."""


@dataclass(frozen=True)
class Repository(Serializable):
    """
    A remote GitHub repository.
    """

    owner: str
    name: str
    ref: str | None

    @property
    def fullname(self) -> str:
        return f"{self.owner}/{self.name}"

    @property
    def url(self: Self) -> str:
        return f"https://github.com/{self.owner}/{self.name}"

    async def clone(self: Self, checkout_dir: Path) -> ClonedRepository:
        """
        Shallow clone this repository
        """
        if checkout_dir.exists():
            logger.debug(f"Reusing cached {self.fullname}")

            if self.ref:
                logger.debug(f"Checking out {self.fullname} @ {self.ref}")

                process = await create_subprocess_exec(
                    *["git", "checkout", "-f", self.ref],
                    cwd=checkout_dir,
                    env={"GIT_TERMINAL_PROMPT": "0"},
                    stdout=PIPE,
                    stderr=PIPE,
                )
                if await process.wait() != 0:
                    _, stderr = await process.communicate()
                    raise ProjectSetupError(
                        f"Failed to checkout {self.ref}: {stderr.decode()}"
                    )

            cloned_repo = await ClonedRepository.from_path(checkout_dir, self)
            await cloned_repo.reset()

            logger.debug(f"Pulling latest changes for {self.fullname} @ {self.ref}")
            await cloned_repo.pull()

            return cloned_repo

        logger.debug(f"Cloning {self.owner}:{self.name} to {checkout_dir}")
        command = [
            "git",
            "clone",
            "--config",
            "advice.detachedHead=false",
            "--quiet",
            "--depth",
            "1",
            "--no-tags",
        ]
        if self.ref:
            command.extend(["--branch", self.ref])

        command.extend(
            [
                f"https://github.com/{self.owner}/{self.name}",
                str(checkout_dir),
            ],
        )

        process = await create_subprocess_exec(
            *command, env={"GIT_TERMINAL_PROMPT": "0"}
        )

        status_code = await process.wait()

        logger.debug(
            f"Finished cloning {self.fullname} with status {status_code}",
        )

        # Configure git user — needed for `self.commit` to work
        await (
            await create_subprocess_exec(
                *["git", "config", "user.email", "ecosystem@astral.sh"],
                cwd=checkout_dir,
                env={"GIT_TERMINAL_PROMPT": "0"},
                stdout=DEVNULL,
                stderr=DEVNULL,
            )
        ).wait()

        await (
            await create_subprocess_exec(
                *["git", "config", "user.name", "Ecosystem Bot"],
                cwd=checkout_dir,
                env={"GIT_TERMINAL_PROMPT": "0"},
                stdout=DEVNULL,
                stderr=DEVNULL,
            )
        ).wait()

        return await ClonedRepository.from_path(checkout_dir, self)


@dataclass(frozen=True)
class ClonedRepository(Repository, Serializable):
    """
    A cloned GitHub repository, which includes the hash of the current commit.
    """

    commit_hash: str
    path: Path

    def url_for(
        self: Self,
        path: str,
        line_number: int | None = None,
        end_line_number: int | None = None,
    ) -> str:
        """
        Return the remote GitHub URL for the given path in this repository.
        """
        url = f"https://github.com/{self.owner}/{self.name}/blob/{self.commit_hash}/{path}"
        if line_number:
            url += f"#L{line_number}"
        if end_line_number:
            url += f"-L{end_line_number}"
        return url

    @property
    def url(self: Self) -> str:
        return f"https://github.com/{self.owner}/{self.name}@{self.commit_hash}"

    @classmethod
    async def from_path(cls, path: Path, repo: Repository):
        return cls(
            name=repo.name,
            owner=repo.owner,
            ref=repo.ref,
            path=path,
            commit_hash=await cls._get_head_commit(path),
        )

    @staticmethod
    async def _get_head_commit(checkout_dir: Path) -> str:
        """
        Return the commit sha for the repository in the checkout directory.
        """
        process = await create_subprocess_exec(
            *["git", "rev-parse", "HEAD"],
            cwd=checkout_dir,
            stdout=PIPE,
        )
        stdout, _ = await process.communicate()
        if await process.wait() != 0:
            raise ProjectSetupError(f"Failed to retrieve commit sha at {checkout_dir}")

        return stdout.decode().strip()

    async def reset(self: Self) -> None:
        """
        Reset the cloned repository to the ref it started at.
        """
        process = await create_subprocess_exec(
            *["git", "reset", "--hard", "origin/" + self.ref] if self.ref else [],
            cwd=self.path,
            env={"GIT_TERMINAL_PROMPT": "0"},
            stdout=PIPE,
            stderr=PIPE,
        )
        _, stderr = await process.communicate()
        if await process.wait() != 0:
            raise RuntimeError(f"Failed to reset: {stderr.decode()}")

    async def pull(self: Self) -> None:
        """
        Pull the latest changes.

        Typically `reset` should be run first.
        """
        process = await create_subprocess_exec(
            *["git", "pull"],
            cwd=self.path,
            env={"GIT_TERMINAL_PROMPT": "0"},
            stdout=PIPE,
            stderr=PIPE,
        )
        _, stderr = await process.communicate()
        if await process.wait() != 0:
            raise RuntimeError(f"Failed to pull: {stderr.decode()}")

    async def commit(self: Self, message: str) -> str:
        """
        Commit all current changes.

        Empty commits are allowed.
        """
        process = await create_subprocess_exec(
            *["git", "commit", "--allow-empty", "-a", "-m", message],
            cwd=self.path,
            env={"GIT_TERMINAL_PROMPT": "0"},
            stdout=PIPE,
            stderr=PIPE,
        )
        _, stderr = await process.communicate()
        if await process.wait() != 0:
            raise RuntimeError(f"Failed to commit: {stderr.decode()}")

        return await self._get_head_commit(self.path)

    async def diff(self: Self, *args: str) -> list[str]:
        """
        Get the current diff from git.

        Arguments are passed to `git diff ...`
        """
        process = await create_subprocess_exec(
            *["git", "diff", *args],
            cwd=self.path,
            env={"GIT_TERMINAL_PROMPT": "0"},
            stdout=PIPE,
            stderr=PIPE,
        )
        stdout, stderr = await process.communicate()
        if await process.wait() != 0:
            raise RuntimeError(f"Failed to commit: {stderr.decode()}")

        return stdout.decode().splitlines()
