import asyncio
import collections
import contextlib
import dataclasses
import functools
import os
import queue
import re
import shelve
import types
import weakref


def takes_preview_generics(
    future: asyncio.Future[int],
    task: asyncio.Task[str],
    deque_object: collections.deque[int],
    defaultdict_object: collections.defaultdict[str, int],
    ordered_dict: collections.OrderedDict[str, int],
    counter_obj: collections.Counter[str],
    chain_map: collections.ChainMap[str, int],
    context_manager: contextlib.AbstractContextManager[str],
    async_context_manager: contextlib.AbstractAsyncContextManager[int],
    dataclass_field: dataclasses.Field[int],
    cached_prop: functools.cached_property[int],
    partial_method: functools.partialmethod[int],
    path_like: os.PathLike[str],
    lifo_queue: queue.LifoQueue[int],
    regular_queue: queue.Queue[int],
    priority_queue: queue.PriorityQueue[int],
    simple_queue: queue.SimpleQueue[int],
    regex_pattern: re.Pattern[str],
    regex_match: re.Match[str],
    bsd_db_shelf: shelve.BsdDbShelf[str, int],
    db_filename_shelf: shelve.DbfilenameShelf[str, int],
    shelf_obj: shelve.Shelf[str, int],
    mapping_proxy: types.MappingProxyType[str, int],
    weak_key_dict: weakref.WeakKeyDictionary[object, int],
    weak_method: weakref.WeakMethod[int],
    weak_set: weakref.WeakSet[int],
    weak_value_dict: weakref.WeakValueDictionary[object, int],
) -> None:
    ...
