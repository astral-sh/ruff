from __future__ import annotations

from collections.abc import Coroutine
from functools import wraps
from typing import Any, Callable

from typing_extensions import ParamSpec, TypeVar

P = ParamSpec("P")
R = TypeVar("R", infer_variance=True)


def inject_client(
    fn: Callable[P, Coroutine[Any, Any, R]],
) -> Callable[P, Coroutine[Any, Any, R]]:
    @wraps(fn)
    async def with_injected_client(*args: P.args, **kwargs: P.kwargs) -> R:
        return await fn(*args, **kwargs)

    return with_injected_client
