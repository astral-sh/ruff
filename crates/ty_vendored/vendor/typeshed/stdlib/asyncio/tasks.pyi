import sys


from . import _CoroutineLike

# Keep asyncio.__all__ updated with any changes to __all__ here
if sys.version_info >= (3, 12):
    __all__ = (
        "Task",
        "create_task",
        "FIRST_COMPLETED",
        "FIRST_EXCEPTION",
        "ALL_COMPLETED",
        "wait",
        "wait_for",
        "as_completed",
        "sleep",
        "gather",
        "shield",
        "ensure_future",
        "run_coroutine_threadsafe",
        "current_task",
        "all_tasks",
        "create_eager_task_factory",
        "eager_task_factory",
        "_register_task",
        "_unregister_task",
        "_enter_task",
        "_leave_task",
    )
else:
    __all__ = (
        "Task",
        "create_task",
        "FIRST_COMPLETED",
        "FIRST_EXCEPTION",
        "ALL_COMPLETED",
        "wait",
        "wait_for",
        "as_completed",
        "sleep",
        "gather",
        "shield",
        "ensure_future",
        "run_coroutine_threadsafe",
        "current_task",
        "all_tasks",
        "_register_task",
        "_unregister_task",
        "_enter_task",
        "_leave_task",
    )



if sys.version_info >= (3, 10):
    def shield(arg: _FutureLike[_T]) -> Future[_T]: ...

else:
    def shield(arg: _FutureLike[_T], *, loop: AbstractEventLoop | None = None) -> Future[_T]: ...


