"""
Abstractions and utilities for working with projects to run ecosystem checks on.
"""

from __future__ import annotations

import abc
import contextlib
import dataclasses
from asyncio import create_subprocess_exec
from dataclasses import dataclass, field
from enum import Enum
from functools import cache
from pathlib import Path
from subprocess import DEVNULL, PIPE
from typing import Any, Self

import tomli
import tomli_w

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
    config_overrides: ConfigOverrides = field(default_factory=lambda: ConfigOverrides())

    def with_preview_enabled(self: Self) -> Self:
        return type(self)(
            repo=self.repo,
            check_options=self.check_options.with_options(preview=True),
            format_options=self.format_options.with_options(preview=True),
            config_overrides=self.config_overrides,
        )

    def __post_init__(self):
        # Convert bare dictionaries for `config_overrides` into the correct type
        if isinstance(self.config_overrides, dict):
            # Bypass the frozen attribute
            object.__setattr__(
                self, "config_overrides", ConfigOverrides(always=self.config_overrides)
            )


ALWAYS_CONFIG_OVERRIDES = {
    # Always unset the required version or we'll fail
    "required-version": None
}


@dataclass(frozen=True)
class ConfigOverrides(Serializable):
    """
    A collection of key, value pairs to override in the Ruff configuration file.

    The key describes a member to override in the toml file; '.' may be used to indicate a
    nested value e.g. `format.quote-style`.

    If a Ruff configuration file does not exist and overrides are provided, it will be createad.
    """

    always: dict[str, Any] = field(default_factory=dict)
    when_preview: dict[str, Any] = field(default_factory=dict)
    when_no_preview: dict[str, Any] = field(default_factory=dict)

    def __hash__(self) -> int:
        # Avoid computing this hash repeatedly since this object is intended
        # to be immutable and serializing to toml is not necessarily cheap
        @cache
        def as_string():
            return tomli_w.dumps(
                {
                    "always": self.always,
                    "when_preview": self.when_preview,
                    "when_no_preview": self.when_no_preview,
                }
            )

        return hash(as_string())

    @contextlib.contextmanager
    def patch_config(
        self,
        dirpath: Path,
        preview: bool,
    ) -> None:
        """
        Temporarily patch the Ruff configuration file in the given directory.
        """
        dot_ruff_toml = dirpath / ".ruff.toml"
        ruff_toml = dirpath / "ruff.toml"
        pyproject_toml = dirpath / "pyproject.toml"

        # Prefer `ruff.toml` over `pyproject.toml`
        if dot_ruff_toml.exists():
            path = dot_ruff_toml
            base = []
        elif ruff_toml.exists():
            path = ruff_toml
            base = []
        else:
            path = pyproject_toml
            base = ["tool", "ruff"]

        overrides = {
            **ALWAYS_CONFIG_OVERRIDES,
            **self.always,
            **(self.when_preview if preview else self.when_no_preview),
        }

        if not overrides:
            yield
            return

        # Read the existing content if the file is present
        if path.exists():
            contents = path.read_text()
            toml = tomli.loads(contents)
        else:
            contents = None
            toml = {}

            # Do not write a toml file if it does not exist and we're just nulling values
            if all((value is None for value in overrides.values())):
                yield
                return

        # Update the TOML, using `.` to descend into nested keys
        for key, value in overrides.items():
            if value is not None:
                logger.debug(f"Setting {key}={value!r} in {path}")
            else:
                logger.debug(f"Restoring {key} to default in {path}")

            target = toml
            names = base + key.split(".")
            for name in names[:-1]:
                if name not in target:
                    target[name] = {}
                target = target[name]

            if value is None:
                # Remove null values i.e. restore to default
                target.pop(names[-1], None)
            else:
                target[names[-1]] = value

        tomli_w.dump(toml, path.open("wb"))

        try:
            yield
        finally:
            # Restore the contents or delete the file
            if contents is None:
                path.unlink()
            else:
                path.write_text(contents)


class RuffCommand(Enum):
    check = "check"
    format = "format"


@dataclass(frozen=True)
class CommandOptions(Serializable, abc.ABC):
    preview: bool = False

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

    # Generating fixes is slow and verbose
    show_fixes: bool = False

    # Limit the number of reported lines per rule
    max_lines_per_rule: int | None = 50

    def to_ruff_args(self) -> list[str]:
        args = [
            "check",
            "--no-cache",
            "--exit-zero",
            # Ignore internal test rules
            "--ignore",
            "RUF9",
            # Use the concise format for comparing violations
            "--output-format",
            "concise",
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
            *command,
            env={"GIT_TERMINAL_PROMPT": "0"},
            stdout=PIPE,
            stderr=PIPE,
        )

        if await process.wait() != 0:
            _, stderr = await process.communicate()
            raise ProjectSetupError(
                f"Failed to clone {self.fullname}: {stderr.decode()}"
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
