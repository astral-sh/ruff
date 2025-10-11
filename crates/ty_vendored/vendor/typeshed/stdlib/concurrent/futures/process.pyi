"""Implements ProcessPoolExecutor.

The following diagram and text describe the data-flow through the system:

|======================= In-process =====================|== Out-of-process ==|

+----------+     +----------+       +--------+     +-----------+    +---------+
|          |  => | Work Ids |       |        |     | Call Q    |    | Process |
|          |     +----------+       |        |     +-----------+    |  Pool   |
|          |     | ...      |       |        |     | ...       |    +---------+
|          |     | 6        |    => |        |  => | 5, call() | => |         |
|          |     | 7        |       |        |     | ...       |    |         |
| Process  |     | ...      |       | Local  |     +-----------+    | Process |
|  Pool    |     +----------+       | Worker |                      |  #1..n  |
| Executor |                        | Thread |                      |         |
|          |     +----------- +     |        |     +-----------+    |         |
|          | <=> | Work Items | <=> |        | <=  | Result Q  | <= |         |
|          |     +------------+     |        |     +-----------+    |         |
|          |     | 6: call()  |     |        |     | ...       |    |         |
|          |     |    future  |     |        |     | 4, result |    |         |
|          |     | ...        |     |        |     | 3, except |    |         |
+----------+     +------------+     +--------+     +-----------+    +---------+

Executor.submit() called:
- creates a uniquely numbered _WorkItem and adds it to the "Work Items" dict
- adds the id of the _WorkItem to the "Work Ids" queue

Local worker thread:
- reads work ids from the "Work Ids" queue and looks up the corresponding
  WorkItem from the "Work Items" dict: if the work item has been cancelled then
  it is simply removed from the dict, otherwise it is repackaged as a
  _CallItem and put in the "Call Q". New _CallItems are put in the "Call Q"
  until "Call Q" is full. NOTE: the size of the "Call Q" is kept small because
  calls placed in the "Call Q" can no longer be cancelled with Future.cancel().
- reads _ResultItems from "Result Q", updates the future stored in the
  "Work Items" dict and deletes the dict entry

Process #1..n:
- reads _CallItems from "Call Q", executes the calls, and puts the resulting
  _ResultItems in "Result Q"
"""

import sys
from collections.abc import Callable, Generator, Iterable, Mapping, MutableMapping, MutableSequence
from multiprocessing.connection import Connection
from multiprocessing.context import BaseContext, Process
from multiprocessing.queues import Queue, SimpleQueue
from threading import Lock, Semaphore, Thread
from types import TracebackType
from typing import Any, Final, Generic, TypeVar, overload
from typing_extensions import TypeVarTuple, Unpack
from weakref import ref

from ._base import BrokenExecutor, Executor, Future

_T = TypeVar("_T")
_Ts = TypeVarTuple("_Ts")

_threads_wakeups: MutableMapping[Any, Any]
_global_shutdown: bool

class _ThreadWakeup:
    _closed: bool
    # Any: Unused send and recv methods
    _reader: Connection[Any, Any]
    _writer: Connection[Any, Any]
    def close(self) -> None: ...
    def wakeup(self) -> None: ...
    def clear(self) -> None: ...

def _python_exit() -> None: ...

EXTRA_QUEUED_CALLS: Final = 1

_MAX_WINDOWS_WORKERS: Final = 61

class _RemoteTraceback(Exception):
    tb: str
    def __init__(self, tb: TracebackType) -> None: ...

class _ExceptionWithTraceback:
    exc: BaseException
    tb: TracebackType
    def __init__(self, exc: BaseException, tb: TracebackType) -> None: ...
    def __reduce__(self) -> str | tuple[Any, ...]: ...

def _rebuild_exc(exc: Exception, tb: str) -> Exception: ...

class _WorkItem(Generic[_T]):
    future: Future[_T]
    fn: Callable[..., _T]
    args: Iterable[Any]
    kwargs: Mapping[str, Any]
    def __init__(self, future: Future[_T], fn: Callable[..., _T], args: Iterable[Any], kwargs: Mapping[str, Any]) -> None: ...

class _ResultItem:
    work_id: int
    exception: Exception
    result: Any
    if sys.version_info >= (3, 11):
        exit_pid: int | None
        def __init__(
            self, work_id: int, exception: Exception | None = None, result: Any | None = None, exit_pid: int | None = None
        ) -> None: ...
    else:
        def __init__(self, work_id: int, exception: Exception | None = None, result: Any | None = None) -> None: ...

class _CallItem:
    work_id: int
    fn: Callable[..., Any]
    args: Iterable[Any]
    kwargs: Mapping[str, Any]
    def __init__(self, work_id: int, fn: Callable[..., Any], args: Iterable[Any], kwargs: Mapping[str, Any]) -> None: ...

class _SafeQueue(Queue[Future[Any]]):
    """Safe Queue set exception to the future object linked to a job"""

    pending_work_items: dict[int, _WorkItem[Any]]
    if sys.version_info < (3, 12):
        shutdown_lock: Lock
    thread_wakeup: _ThreadWakeup
    if sys.version_info >= (3, 12):
        def __init__(
            self,
            max_size: int | None = 0,
            *,
            ctx: BaseContext,
            pending_work_items: dict[int, _WorkItem[Any]],
            thread_wakeup: _ThreadWakeup,
        ) -> None: ...
    else:
        def __init__(
            self,
            max_size: int | None = 0,
            *,
            ctx: BaseContext,
            pending_work_items: dict[int, _WorkItem[Any]],
            shutdown_lock: Lock,
            thread_wakeup: _ThreadWakeup,
        ) -> None: ...

    def _on_queue_feeder_error(self, e: Exception, obj: _CallItem) -> None: ...

def _get_chunks(*iterables: Any, chunksize: int) -> Generator[tuple[Any, ...], None, None]:
    """Iterates over zip()ed iterables in chunks."""

def _process_chunk(fn: Callable[..., _T], chunk: Iterable[tuple[Any, ...]]) -> list[_T]:
    """Processes a chunk of an iterable passed to map.

    Runs the function passed to map() on a chunk of the
    iterable passed to map.

    This function is run in a separate process.

    """

if sys.version_info >= (3, 11):
    def _sendback_result(
        result_queue: SimpleQueue[_WorkItem[Any]],
        work_id: int,
        result: Any | None = None,
        exception: Exception | None = None,
        exit_pid: int | None = None,
    ) -> None:
        """Safely send back the given result or exception"""

else:
    def _sendback_result(
        result_queue: SimpleQueue[_WorkItem[Any]], work_id: int, result: Any | None = None, exception: Exception | None = None
    ) -> None:
        """Safely send back the given result or exception"""

if sys.version_info >= (3, 11):
    def _process_worker(
        call_queue: Queue[_CallItem],
        result_queue: SimpleQueue[_ResultItem],
        initializer: Callable[[Unpack[_Ts]], object] | None,
        initargs: tuple[Unpack[_Ts]],
        max_tasks: int | None = None,
    ) -> None:
        """Evaluates calls from call_queue and places the results in result_queue.

        This worker is run in a separate process.

        Args:
            call_queue: A ctx.Queue of _CallItems that will be read and
                evaluated by the worker.
            result_queue: A ctx.Queue of _ResultItems that will written
                to by the worker.
            initializer: A callable initializer, or None
            initargs: A tuple of args for the initializer
        """

else:
    def _process_worker(
        call_queue: Queue[_CallItem],
        result_queue: SimpleQueue[_ResultItem],
        initializer: Callable[[Unpack[_Ts]], object] | None,
        initargs: tuple[Unpack[_Ts]],
    ) -> None:
        """Evaluates calls from call_queue and places the results in result_queue.

        This worker is run in a separate process.

        Args:
            call_queue: A ctx.Queue of _CallItems that will be read and
                evaluated by the worker.
            result_queue: A ctx.Queue of _ResultItems that will written
                to by the worker.
            initializer: A callable initializer, or None
            initargs: A tuple of args for the initializer
        """

class _ExecutorManagerThread(Thread):
    """Manages the communication between this process and the worker processes.

    The manager is run in a local thread.

    Args:
        executor: A reference to the ProcessPoolExecutor that owns
            this thread. A weakref will be own by the manager as well as
            references to internal objects used to introspect the state of
            the executor.
    """

    thread_wakeup: _ThreadWakeup
    shutdown_lock: Lock
    executor_reference: ref[Any]
    processes: MutableMapping[int, Process]
    call_queue: Queue[_CallItem]
    result_queue: SimpleQueue[_ResultItem]
    work_ids_queue: Queue[int]
    pending_work_items: dict[int, _WorkItem[Any]]
    def __init__(self, executor: ProcessPoolExecutor) -> None: ...
    def run(self) -> None: ...
    def add_call_item_to_queue(self) -> None: ...
    def wait_result_broken_or_wakeup(self) -> tuple[Any, bool, str]: ...
    def process_result_item(self, result_item: int | _ResultItem) -> None: ...
    def is_shutting_down(self) -> bool: ...
    def terminate_broken(self, cause: str) -> None: ...
    def flag_executor_shutting_down(self) -> None: ...
    def shutdown_workers(self) -> None: ...
    def join_executor_internals(self) -> None: ...
    def get_n_children_alive(self) -> int: ...

_system_limits_checked: bool
_system_limited: bool | None

def _check_system_limits() -> None: ...
def _chain_from_iterable_of_lists(iterable: Iterable[MutableSequence[Any]]) -> Any:
    """
    Specialized implementation of itertools.chain.from_iterable.
    Each item in *iterable* should be a list.  This function is
    careful not to keep references to yielded objects.
    """

class BrokenProcessPool(BrokenExecutor):
    """
    Raised when a process in a ProcessPoolExecutor terminated abruptly
    while a future was in the running state.
    """

class ProcessPoolExecutor(Executor):
    _mp_context: BaseContext | None
    _initializer: Callable[..., None] | None
    _initargs: tuple[Any, ...]
    _executor_manager_thread: _ThreadWakeup
    _processes: MutableMapping[int, Process]
    _shutdown_thread: bool
    _shutdown_lock: Lock
    _idle_worker_semaphore: Semaphore
    _broken: bool
    _queue_count: int
    _pending_work_items: dict[int, _WorkItem[Any]]
    _cancel_pending_futures: bool
    _executor_manager_thread_wakeup: _ThreadWakeup
    _result_queue: SimpleQueue[Any]
    _work_ids: Queue[Any]
    if sys.version_info >= (3, 11):
        @overload
        def __init__(
            self,
            max_workers: int | None = None,
            mp_context: BaseContext | None = None,
            initializer: Callable[[], object] | None = None,
            initargs: tuple[()] = (),
            *,
            max_tasks_per_child: int | None = None,
        ) -> None:
            """Initializes a new ProcessPoolExecutor instance.

            Args:
                max_workers: The maximum number of processes that can be used to
                    execute the given calls. If None or not given then as many
                    worker processes will be created as the machine has processors.
                mp_context: A multiprocessing context to launch the workers created
                    using the multiprocessing.get_context('start method') API. This
                    object should provide SimpleQueue, Queue and Process.
                initializer: A callable used to initialize worker processes.
                initargs: A tuple of arguments to pass to the initializer.
                max_tasks_per_child: The maximum number of tasks a worker process
                    can complete before it will exit and be replaced with a fresh
                    worker process. The default of None means worker process will
                    live as long as the executor. Requires a non-'fork' mp_context
                    start method. When given, we default to using 'spawn' if no
                    mp_context is supplied.
            """

        @overload
        def __init__(
            self,
            max_workers: int | None = None,
            mp_context: BaseContext | None = None,
            *,
            initializer: Callable[[Unpack[_Ts]], object],
            initargs: tuple[Unpack[_Ts]],
            max_tasks_per_child: int | None = None,
        ) -> None: ...
        @overload
        def __init__(
            self,
            max_workers: int | None,
            mp_context: BaseContext | None,
            initializer: Callable[[Unpack[_Ts]], object],
            initargs: tuple[Unpack[_Ts]],
            *,
            max_tasks_per_child: int | None = None,
        ) -> None: ...
    else:
        @overload
        def __init__(
            self,
            max_workers: int | None = None,
            mp_context: BaseContext | None = None,
            initializer: Callable[[], object] | None = None,
            initargs: tuple[()] = (),
        ) -> None:
            """Initializes a new ProcessPoolExecutor instance.

            Args:
                max_workers: The maximum number of processes that can be used to
                    execute the given calls. If None or not given then as many
                    worker processes will be created as the machine has processors.
                mp_context: A multiprocessing context to launch the workers. This
                    object should provide SimpleQueue, Queue and Process.
                initializer: A callable used to initialize worker processes.
                initargs: A tuple of arguments to pass to the initializer.
            """

        @overload
        def __init__(
            self,
            max_workers: int | None = None,
            mp_context: BaseContext | None = None,
            *,
            initializer: Callable[[Unpack[_Ts]], object],
            initargs: tuple[Unpack[_Ts]],
        ) -> None: ...
        @overload
        def __init__(
            self,
            max_workers: int | None,
            mp_context: BaseContext | None,
            initializer: Callable[[Unpack[_Ts]], object],
            initargs: tuple[Unpack[_Ts]],
        ) -> None: ...

    def _start_executor_manager_thread(self) -> None: ...
    def _adjust_process_count(self) -> None: ...

    if sys.version_info >= (3, 14):
        def kill_workers(self) -> None:
            """Attempts to kill the executor's workers.
            Iterates through all of the current worker processes and kills
            each one that is still alive.

            After killing workers, the pool will be in a broken state
            and no longer usable (for instance, new tasks should not be
            submitted).
            """

        def terminate_workers(self) -> None:
            """Attempts to terminate the executor's workers.
            Iterates through all of the current worker processes and terminates
            each one that is still alive.

            After terminating workers, the pool will be in a broken state
            and no longer usable (for instance, new tasks should not be
            submitted).
            """
