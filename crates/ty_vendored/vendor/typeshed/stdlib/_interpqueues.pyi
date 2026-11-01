"""This module provides primitive operations to manage Python interpreters.
The 'interpreters' module provides a more convenient interface.
"""
import sys
from typing import Any, Literal, SupportsIndex, TypeAlias

_UnboundOp: TypeAlias = Literal[1, 2, 3]

class QueueError(RuntimeError):
    """Indicates that a queue-related error happened."""
class QueueNotFoundError(QueueError): ...

def bind(qid: SupportsIndex) -> None:
    """Take a reference to the identified queue.

The queue is not destroyed until there are no references left.
"""

if sys.version_info >= (3, 15):
    def create(maxsize: SupportsIndex, unboundop: SupportsIndex = -1, fallback: SupportsIndex = -1) -> int:
        """Create a new cross-interpreter queue and return its unique generated ID.

It is a new reference as though bind() had been called on the queue.
The caller is responsible for calling destroy() for the new queue
before the runtime is finalized.
"""

else:
    def create(maxsize: SupportsIndex, fmt: SupportsIndex, unboundop: _UnboundOp) -> int:
        """create(maxsize, unboundop, fallback) -> qid

Create a new cross-interpreter queue and return its unique generated ID.
It is a new reference as though bind() had been called on the queue.

The caller is responsible for calling destroy() for the new queue
before the runtime is finalized.
"""

def destroy(qid: SupportsIndex) -> None:
    """Clear and destroy the queue.

Afterward attempts to use the queue will behave as though it never
existed.
"""
def get(qid: SupportsIndex) -> tuple[Any, int, _UnboundOp | None]:
    """Return the (object, unbound op) from the front of the queue.

If there is nothing to receive then raise QueueEmpty.
"""
def get_count(qid: SupportsIndex) -> int:
    """Return the number of items in the queue."""
def get_maxsize(qid: SupportsIndex) -> int:
    """Return the maximum number of items in the queue."""
def get_queue_defaults(qid: SupportsIndex) -> tuple[int, _UnboundOp]:
    """Return the queue's default values, set when it was created."""
def is_full(qid: SupportsIndex) -> bool:
    """Return true if the queue has a maxsize and has reached it."""
def list_all() -> list[tuple[int, int, _UnboundOp]]:
    """Return the list of ID triples for all queues.

Each ID triple consists of (ID, default unbound op, default fallback).
"""

if sys.version_info >= (3, 15):
    def put(qid: SupportsIndex, obj: Any, unboundop: SupportsIndex = -1, fallback: SupportsIndex = -1) -> None:
        """Add the object's data to the queue."""

else:
    def put(qid: SupportsIndex, obj: Any, fmt: SupportsIndex, unboundop: _UnboundOp) -> None:
        """put(qid, obj)

Add the object's data to the queue.
"""

def release(qid: SupportsIndex) -> None:
    """Release a reference to the queue.

The queue is destroyed once there are no references left.
"""
