import asyncio
import inspect
from collections.abc import Coroutine
from functools import wraps
from typing import TYPE_CHECKING, Any, Callable, TypeVar, Union, cast

from typing_extensions import ParamSpec

if TYPE_CHECKING:
    from prefect.flows import Flow
    from prefect.tasks import Task

R = TypeVar("R")
P = ParamSpec("P")


def is_in_async_context() -> bool:
    """
    Returns True if called from within an async context.

    An async context is one of:
        - a coroutine
        - a running event loop
        - a task or flow that is async
    """
    from prefect.context import get_run_context
    from prefect.exceptions import MissingContextError

    try:
        run_ctx = get_run_context()
        parent_obj = getattr(run_ctx, "task", None)
        if not parent_obj:
            parent_obj = getattr(run_ctx, "flow", None)
        return getattr(parent_obj, "isasync", True)
    except MissingContextError:
        # not in an execution context, make best effort to
        # decide whether to syncify
        try:
            asyncio.get_running_loop()
            return True
        except RuntimeError:
            return False


def _is_acceptable_callable(
    obj: Union[
        Callable[P, R], "Flow[P, R]", "Task[P, R]", "classmethod[type[Any], P, R]"
    ],
) -> bool:
    if inspect.iscoroutinefunction(obj):
        return True

    # Check if a task or flow. Need to avoid importing `Task` or `Flow` here
    # due to circular imports.
    if (fn := getattr(obj, "fn", None)) and inspect.iscoroutinefunction(fn):
        return True

    if isinstance(obj, classmethod) and inspect.iscoroutinefunction(obj.__func__):
        return True

    return False


def async_dispatch(
    async_impl: Union[
        Callable[P, Coroutine[Any, Any, R]],
        "classmethod[type[Any], P, Coroutine[Any, Any, R]]",
    ],
) -> Callable[[Callable[P, R]], Callable[P, Union[R, Coroutine[Any, Any, R]]]]:
    """
    Decorator that dispatches to either sync or async implementation based on context.

    Args:
        async_impl: The async implementation to dispatch to when in async context
    """
    if not _is_acceptable_callable(async_impl):
        raise TypeError("async_impl must be an async function")
    if isinstance(async_impl, classmethod):
        async_impl = cast(Callable[P, Coroutine[Any, Any, R]], async_impl.__func__)

    def decorator(
        sync_fn: Callable[P, R],
    ) -> Callable[P, Union[R, Coroutine[Any, Any, R]]]:
        @wraps(sync_fn)
        def wrapper(
            *args: P.args,
            **kwargs: P.kwargs,
        ) -> Union[R, Coroutine[Any, Any, R]]:
            _sync = kwargs.pop("_sync", None)
            should_run_sync = (
                bool(_sync) if _sync is not None else not is_in_async_context()
            )
            fn = sync_fn if should_run_sync else async_impl
            return fn(*args, **kwargs)

        # Add the .aio attribute for compatibility with existing code that expects it
        # (e.g., CLI commands, tests that mock .aio)
        wrapper.aio = async_impl  # type: ignore
        return wrapper

    return decorator
