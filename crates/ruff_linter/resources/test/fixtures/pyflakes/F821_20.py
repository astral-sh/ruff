"""Test lazy evaluation of type alias values."""

type RecordCallback[R: Record] = Callable[[R], None]

from collections.abc import Callable
