from ruff_ecosystem.models import Repository, ClonedRepository
from contextlib import asynccontextmanager
from pathlib import Path
from typing import AsyncGenerator
from asyncio import create_subprocess_exec
from subprocess import PIPE
from ruff_ecosystem import logger


@asynccontextmanager
async def clone(
    repo: Repository, checkout_dir: Path
) -> AsyncGenerator[ClonedRepository, None]:
    """Shallow clone this repository to a temporary directory."""
    if checkout_dir.exists():
        logger.debug(f"Reusing {repo.owner}:{repo.name}")
        yield await _cloned_repository(repo, checkout_dir)
        return

    logger.debug(f"Cloning {repo.owner}:{repo.name} to {checkout_dir}")
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
    if repo.branch:
        command.extend(["--branch", repo.branch])

    command.extend(
        [
            f"https://github.com/{repo.owner}/{repo.name}",
            checkout_dir,
        ],
    )

    process = await create_subprocess_exec(*command, env={"GIT_TERMINAL_PROMPT": "0"})

    status_code = await process.wait()

    logger.debug(
        f"Finished cloning {repo.fullname} with status {status_code}",
    )
    yield await _cloned_repository(repo, checkout_dir)


async def _cloned_repository(repo: Repository, checkout_dir: Path) -> ClonedRepository:
    return ClonedRepository(
        name=repo.name,
        owner=repo.owner,
        branch=repo.branch,
        path=checkout_dir,
        commit_hash=await _get_commit_hash(checkout_dir),
    )


async def _get_commit_hash(checkout_dir: Path) -> str:
    """
    Return the commit sha for the repository in the checkout directory.
    """
    process = await create_subprocess_exec(
        *["git", "rev-parse", "HEAD"],
        cwd=checkout_dir,
        stdout=PIPE,
    )
    stdout, _ = await process.communicate()
    assert await process.wait() == 0, f"Failed to retrieve commit sha at {checkout_dir}"
    return stdout.decode().strip()
