from __future__ import annotations

import asyncio
import contextlib


async def routine(): ...

task_a = asyncio.create_task(routine())
task_b = asyncio.create_task(routine())


async def main():
    # Good

    _ = await asyncio.gather(task_a, task_b)
    _ = await asyncio.gather(task_a, task_b, return_exceptions=True)
    await asyncio.gather(task_a, task_b)
    await asyncio.gather(task_a, task_b, return_exceptions=False)

    with contextlib.suppress(ValueError):
        _ = await asyncio.gather(task_a, task_b)

    # Bad

    await asyncio.gather(task_a, task_b, return_exceptions=True)  # RUF071

    # Undetected

    exceptions = True
    await asyncio.gather(task_a, task_b, return_exceptions=exceptions)
