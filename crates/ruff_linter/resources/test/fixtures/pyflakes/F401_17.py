"""Test that runtime typing references are properly attributed to scoped imports."""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

if TYPE_CHECKING:
    from threading import Thread


def fn(thread: Thread):
    from threading import Thread

    # The `Thread` on the left-hand side should resolve to the `Thread` imported at the
    # top level.
    x: Thread


def fn(thread: Thread):
    from threading import Thread

    # The `Thread` on the left-hand side should resolve to the `Thread` imported at the
    # top level.
    cast("Thread", thread)


def fn(thread: Thread):
    from threading import Thread

    # The `Thread` on the right-hand side should resolve to the`Thread` imported within
    # `fn`.
    cast(Thread, thread)
