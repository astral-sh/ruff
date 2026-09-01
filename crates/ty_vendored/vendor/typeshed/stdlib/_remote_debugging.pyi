from _typeshed import StrOrBytesPath, structseq
from collections.abc import Callable
from typing import Final, TypeAlias, final
from typing_extensions import Self

_Location: TypeAlias = tuple[int, int, int, int] | LocationInfo | None
_Frame: TypeAlias = tuple[str, _Location, str, int | None] | FrameInfo
_Stats: TypeAlias = dict[str, int | float]

PROCESS_VM_READV_SUPPORTED: Final[int]
THREAD_STATUS_GIL_REQUESTED: Final[int]
THREAD_STATUS_HAS_EXCEPTION: Final[int]
THREAD_STATUS_HAS_GIL: Final[int]
THREAD_STATUS_MAIN_THREAD: Final[int]
THREAD_STATUS_ON_CPU: Final[int]
THREAD_STATUS_UNKNOWN: Final[int]

@final
class LocationInfo(structseq[int], tuple[int, int, int, int]):
    """Source location information: (lineno, end_lineno, col_offset, end_col_offset)"""
    __match_args__: Final = ("lineno", "end_lineno", "col_offset", "end_col_offset")
    @property
    def lineno(self) -> int:
        """Line number"""
    @property
    def end_lineno(self) -> int:
        """End line number"""
    @property
    def col_offset(self) -> int:
        """Column offset"""
    @property
    def end_col_offset(self) -> int:
        """End column offset"""

@final
class FrameInfo(structseq[object], tuple[str, _Location, str, int | None]):
    """Information about a frame"""
    __match_args__: Final = ("filename", "location", "funcname", "opcode")
    @property
    def filename(self) -> str:
        """Source code filename"""
    @property
    def location(self) -> _Location:
        """LocationInfo structseq or None for synthetic frames"""
    @property
    def funcname(self) -> str:
        """Function name"""
    @property
    def opcode(self) -> int | None:
        """Opcode being executed (None if not gathered)"""

@final
class CoroInfo(structseq[object], tuple[list[_Frame], int | str]):
    """Information about a coroutine"""
    __match_args__: Final = ("call_stack", "task_name")
    @property
    def call_stack(self) -> list[_Frame]:
        """Coroutine call stack"""
    @property
    def task_name(self) -> int | str:
        """Task name"""

@final
class TaskInfo(structseq[object], tuple[int, str, list[CoroInfo], list[CoroInfo]]):
    """Information about an asyncio task"""
    __match_args__: Final = ("task_id", "task_name", "coroutine_stack", "awaited_by")
    @property
    def task_id(self) -> int:
        """Task ID (memory address)"""
    @property
    def task_name(self) -> str:
        """Task name"""
    @property
    def coroutine_stack(self) -> list[CoroInfo]:
        """Coroutine call stack"""
    @property
    def awaited_by(self) -> list[CoroInfo]:
        """Tasks awaiting this task"""

@final
class ThreadInfo(structseq[object], tuple[int, int, list[_Frame]]):
    """Information about a thread"""
    __match_args__: Final = ("thread_id", "status", "frame_info")
    @property
    def thread_id(self) -> int:
        """Thread ID"""
    @property
    def status(self) -> int:
        """Thread status (flags: HAS_GIL, ON_CPU, UNKNOWN or legacy enum)"""
    @property
    def frame_info(self) -> list[_Frame]:
        """Frame information"""

@final
class InterpreterInfo(structseq[object], tuple[int, list[ThreadInfo]]):
    """Information about an interpreter"""
    __match_args__: Final = ("interpreter_id", "threads")
    @property
    def interpreter_id(self) -> int:
        """Interpreter ID"""
    @property
    def threads(self) -> list[ThreadInfo]:
        """List of threads in this interpreter"""

@final
class AwaitedInfo(structseq[object], tuple[int, list[TaskInfo]]):
    """Information about what a thread is awaiting"""
    __match_args__: Final = ("thread_id", "awaited_by")
    @property
    def thread_id(self) -> int:
        """Thread ID"""
    @property
    def awaited_by(self) -> list[TaskInfo]:
        """List of tasks awaited by this thread"""

@final
class GCStatsInfo(structseq[object], tuple[int, int, int, int, int, int, int, int, int, float]):
    """Information about a garbage collector stats sample"""
    __match_args__: Final = (
        "gen",
        "iid",
        "ts_start",
        "ts_stop",
        "collections",
        "collected",
        "uncollectable",
        "candidates",
        "heap_size",
        "duration",
    )
    @property
    def gen(self) -> int:
        """GC generation number"""
    @property
    def iid(self) -> int:
        """Interpreter ID"""
    @property
    def ts_start(self) -> int:
        """Raw timestamp at collection start"""
    @property
    def ts_stop(self) -> int:
        """Raw timestamp at collection stop"""
    @property
    def collections(self) -> int:
        """Total number of collections"""
    @property
    def collected(self) -> int:
        """Total number of collected objects"""
    @property
    def uncollectable(self) -> int:
        """Total number of uncollectable objects"""
    @property
    def candidates(self) -> int:
        """Total objects considered and traversed"""
    @property
    def heap_size(self) -> int:
        """Number of live objects"""
    @property
    def duration(self) -> float:
        """Total collection time, in seconds"""

@final
class RemoteUnwinder:
    """RemoteUnwinder(pid): Inspect stack of a remote Python process."""
    def __init__(
        self,
        pid: int,
        *,
        all_threads: bool = False,
        only_active_thread: bool = False,
        mode: int = 0,
        debug: bool = False,
        skip_non_matching_threads: bool = True,
        native: bool = False,
        gc: bool = False,
        opcodes: bool = False,
        cache_frames: bool = False,
        stats: bool = False,
    ) -> None: ...
    def get_stack_trace(self) -> list[InterpreterInfo]:
        """Returns stack traces for all interpreters and threads in process.

Each element in the returned list is a tuple of (interpreter_id,
thread_list), where:
- interpreter_id is the interpreter identifier
- thread_list is a list of tuples (thread_id, frame_list) for
  threads in that interpreter
  - thread_id is the OS thread identifier
  - frame_list is a list of tuples (function_name, filename,
    line_number) representing the Python stack frames for that
    thread, ordered from most recent to oldest

The threads returned depend on the initialization parameters:
- If only_active_thread was True: returns only the thread holding
  the GIL across all interpreters
- If all_threads was True: returns all threads across all
  interpreters
- Otherwise: returns only the main thread of each interpreter

Example:
    [
        (0, [  # Main interpreter
            (1234, [
                ('process_data', 'worker.py', 127),
                ('run_worker', 'worker.py', 45),
                ('main', 'app.py', 23)
            ]),
            (1235, [
                ('handle_request', 'server.py', 89),
                ('serve_forever', 'server.py', 52)
            ])
        ]),
        (1, [  # Sub-interpreter
            (1236, [
                ('sub_worker', 'sub.py', 15)
            ])
        ])
    ]

Raises:
    RuntimeError: If there is an error copying memory from the
        target process
    OSError: If there is an error accessing the target process
    PermissionError: If access to the target process is denied
    UnicodeDecodeError: If there is an error decoding strings from
        the target process
"""
    def get_all_awaited_by(self) -> list[AwaitedInfo]:
        """Get all tasks and their awaited_by relationships from the remote process.

This provides a tree structure showing which tasks are waiting for
other tasks.

For each task, returns:
1. The call stack frames leading to where the task is currently
   executing
2. The name of the task
3. A list of tasks that this task is waiting for, with their own
   frames/names/etc

Returns a list of [frames, task_name, subtasks] where:
- frames: List of (func_name, filename, lineno) showing the call
  stack
- task_name: String identifier for the task
- subtasks: List of tasks being awaited by this task, in same format

Raises:
    RuntimeError: If AsyncioDebug section is not available in the
        remote process
    MemoryError: If memory allocation fails
    OSError: If reading from the remote process fails

Example output:
[
    [
        [("c5", "script.py", 10), ("c4", "script.py", 14)],
        "c2_root",
        [
            [
                [("c1", "script.py", 23)],
                "sub_main_2",
                [...]
            ],
            [...]
        ]
    ]
]
"""
    def get_async_stack_trace(self) -> list[AwaitedInfo]:
        """Get the currently running async tasks and their dependency graphs from the remote process.

This returns information about running tasks and all tasks that are
waiting for them, forming a complete dependency graph for each
thread's active task.

For each thread with a running task, returns the running task plus
all tasks that transitively depend on it (tasks waiting for the
running task, tasks waiting for those tasks, etc.).

Returns a list of per-thread results, where each thread result
contains:
- Thread ID
- List of task information for the running task and all its waiters

Each task info contains:
- Task ID (memory address)
- Task name
- Call stack frames: List of (func_name, filename, lineno)
- List of tasks waiting for this task (recursive structure)

Raises:
    RuntimeError: If AsyncioDebug section is not available in the
        target process
    MemoryError: If memory allocation fails
    OSError: If reading from the remote process fails

Example output (similar structure to get_all_awaited_by but only for
running tasks):
[
    (140234, [
        (4345585712, 'main_task',
         [("run_server", "server.py", 127), ("main", "app.py", 23)],
         [
             (4345585800, 'worker_1', [...], [...]),
             (4345585900, 'worker_2', [...], [...])
         ])
    ])
]
"""
    def get_stats(self) -> _Stats:
        """Get collected statistics about profiling performance.

Returns a dictionary containing statistics about cache performance,
memory reads, and other profiling metrics. Only available if the
RemoteUnwinder was created with stats=True.

Returns:
    dict: A dictionary containing:
        - total_samples: Total number of get_stack_trace calls
        - frame_cache_hits: Full cache hits (entire stack unchanged)
        - frame_cache_misses: Cache misses requiring full walk
        - frame_cache_partial_hits: Partial hits (stopped at cached
          frame)
        - frames_read_from_cache: Total frames retrieved from cache
        - frames_read_from_memory: Total frames read from remote
          memory
        - memory_reads: Total remote memory read operations
        - memory_bytes_read: Total bytes read from remote memory
        - code_object_cache_hits: Code object cache hits
        - code_object_cache_misses: Code object cache misses
        - stale_cache_invalidations: Times stale cache entries were
          cleared
        - batched_read_attempts: Batched remote-read attempts
        - batched_read_successes: Attempts that read all requested
          segments
        - batched_read_misses: Attempts that fell back or partially
          read
        - batched_read_segments_requested: Segments requested by
          batched reads
        - batched_read_segments_completed: Segments completed by
          batched reads
        - frame_cache_hit_rate: Percentage of samples that hit the
          cache
        - code_object_cache_hit_rate: Percentage of code object
          lookups that hit cache
        - batched_read_success_rate: Percentage of batched reads
          that completed all segments
        - batched_read_segment_completion_rate: Percentage of
          requested segments read by batched reads

Raises:
    RuntimeError: If stats collection was not enabled (stats=False)
"""
    def pause_threads(self) -> bool:
        """Pause all threads in the target process.

This stops all threads in the target process to allow for consistent
memory reads during sampling. Must be paired with a call to
resume_threads().

Returns True if threads were successfully paused, False if they were
already paused.

Raises:
    RuntimeError: If there is an error stopping the threads
"""
    def resume_threads(self) -> bool:
        """Resume all threads in the target process.

This resumes threads that were previously paused with
pause_threads().

Returns True if threads were successfully resumed, False if they
were not paused.
"""

@final
class GCMonitor:
    """GCMonitor(pid): Monitor GC events of a remote Python process."""
    def __init__(self, pid: int, *, debug: bool = False) -> None: ...
    def get_gc_stats(self, all_interpreters: bool = False) -> list[GCStatsInfo]:
        """Get garbage collector statistics from external Python process.

  all_interpreters
    If True, return GC statistics from all interpreters.
    If False, return only from main interpreter.

Returns a list of GCStatsInfo objects with GC statistics data.

Returns:
    list of GCStatsInfo: A list of stats samples containing:
        - gen: GC generation number.
        - iid: Interpreter ID.
        - ts_start: Raw timestamp at collection start.
        - ts_stop: Raw timestamp at collection stop.
        - collections: Total number of collections.
        - collected: Total number of collected objects.
        - uncollectable: Total number of uncollectable objects.
        - candidates: Total objects considered and traversed.
        - heap_size: number of live objects.
        - duration: Total collection time, in seconds.

Raises:
    RuntimeError: If the target process cannot be inspected or if
        its debug offsets or GC stats layout are incompatible.
"""

@final
class BinaryWriter:
    def __init__(
        self, filename: StrOrBytesPath, sample_interval_us: int, start_time_us: int, *, compression: int = 0
    ) -> None: ...
    @property
    def total_samples(self) -> int:
        """Total samples written"""
    def write_sample(self, stack_frames: list[InterpreterInfo], timestamp_us: int) -> None:
        """Write a sample to the binary file.

Arguments:
    stack_frames: List of InterpreterInfo objects
    timestamp_us: Current timestamp in microseconds (from
        time.monotonic() * 1e6)
"""
    def finalize(self) -> None:
        """Finalize and close the binary file.

Writes string/frame tables, footer, and updates header.
"""
    def close(self) -> None:
        """Close the writer without finalizing (discards data)."""
    def __enter__(self) -> Self:
        """Enter context manager."""
    def __exit__(self, exc_type: object = None, exc_val: object = None, exc_tb: object = None) -> bool:
        """Exit context manager, finalizing the file."""
    def get_stats(self) -> _Stats:
        """Get encoding statistics for the writer.

Returns a dict with encoding statistics including
repeat/full/suffix/pop-push record counts, frames written/saved, and
compression ratio.
"""

@final
class BinaryReader:
    def __init__(self, filename: StrOrBytesPath) -> None: ...
    @property
    def sample_count(self) -> int:
        """Number of samples in file"""
    @property
    def sample_interval_us(self) -> int:
        """Sample interval in microseconds"""
    def replay(self, collector: object, progress_callback: Callable[[int, int], object] | None = None) -> int:
        """Replay samples through a collector.

Arguments:
    collector: Collector object with collect() method
    progress_callback: Optional callable(current, total)

Returns:
    Number of samples replayed
"""
    def get_info(self) -> dict[str, object]:
        """Get metadata about the binary file.

Returns:
    Dict with file metadata
"""
    def get_stats(self) -> _Stats:
        """Get reconstruction statistics from replay.

Returns a dict with statistics about record types decoded and
samples reconstructed during replay.
"""
    def close(self) -> None:
        """Close the reader and free resources."""
    def __enter__(self) -> Self:
        """Enter context manager."""
    def __exit__(self, exc_type: object = None, exc_val: object = None, exc_tb: object = None) -> bool:
        """Exit context manager, closing the file."""

def zstd_available() -> bool:
    """Check if zstd compression is available.

Returns:
    True if zstd available, False otherwise
"""
def get_child_pids(pid: int, *, recursive: bool = True) -> list[int]:
    """Get all child process IDs of the given process.

  pid
    Process ID of the parent process
  recursive
    If True, return all descendants (children, grandchildren, etc.).
    If False, return only direct children.

Returns a list of child process IDs.  Returns an empty list if no
children are found.

This function provides a snapshot of child processes at a moment in
time.  Child processes may exit or new ones may be created after the
list is returned.

Raises:
    OSError: If unable to enumerate processes
    NotImplementedError: If not supported on this platform
"""
def is_python_process(pid: int) -> bool:
    """Check if a process is a Python process."""
def get_gc_stats(pid: int, *, all_interpreters: bool = False) -> list[GCStatsInfo]:
    """Get garbage collector statistics from external Python process.

  all_interpreters
    If True, return GC statistics from all interpreters.
    If False, return only from main interpreter.

Returns:
    list of GCStatsInfo: A list of stats samples containing:
        - gen: GC generation number.
        - iid: Interpreter ID.
        - ts_start: Raw timestamp at collection start.
        - ts_stop: Raw timestamp at collection stop.
        - collections: Total number of collections.
        - collected: Total number of collected objects.
        - uncollectable: Total number of uncollectable objects.
        - candidates: Total objects considered and traversed.
        - duration: Total collection time, in seconds.

Raises:
    RuntimeError: If the target process cannot be inspected or if its
        debug offsets or GC stats layout are incompatible.
"""
