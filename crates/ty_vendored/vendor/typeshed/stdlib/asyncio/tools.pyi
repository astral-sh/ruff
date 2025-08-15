from collections.abc import Iterable
from enum import Enum
from typing import NamedTuple, SupportsIndex, type_check_only

@type_check_only
class _AwaitedInfo(NamedTuple):  # AwaitedInfo_Type from _remote_debugging
    thread_id: int
    awaited_by: list[_TaskInfo]

@type_check_only
class _TaskInfo(NamedTuple):  # TaskInfo_Type from _remote_debugging
    task_id: int
    task_name: str
    coroutine_stack: list[_CoroInfo]
    awaited_by: list[_CoroInfo]

@type_check_only
class _CoroInfo(NamedTuple):  # CoroInfo_Type from _remote_debugging
    call_stack: list[_FrameInfo]
    task_name: int | str

@type_check_only
class _FrameInfo(NamedTuple):  # FrameInfo_Type from _remote_debugging
    filename: str
    lineno: int
    funcname: str

class NodeType(Enum):
    COROUTINE = 1
    TASK = 2

class CycleFoundException(Exception):
    cycles: list[list[int]]
    id2name: dict[int, str]
    def __init__(self, cycles: list[list[int]], id2name: dict[int, str]) -> None: ...

def get_all_awaited_by(pid: SupportsIndex) -> list[_AwaitedInfo]: ...
def build_async_tree(result: Iterable[_AwaitedInfo], task_emoji: str = "(T)", cor_emoji: str = "") -> list[list[str]]: ...
def build_task_table(result: Iterable[_AwaitedInfo]) -> list[list[int | str]]: ...
def display_awaited_by_tasks_table(pid: SupportsIndex) -> None: ...
def display_awaited_by_tasks_tree(pid: SupportsIndex) -> None: ...
