from collections import deque
import collections


def f():
    queue = collections.deque([])  # C421


def f():
    queue = collections.deque([], maxlen=10)  # C421


def f():
    queue = deque([])  # C421


def f():
    queue = deque(())  # C421


def f():
    queue = deque({})  # C421


def f():
    queue = deque(set())  # C421


def f():
    queue = collections.deque([], maxlen=10)  # C421


def f():
    class FakeDeque:
        pass

    deque = FakeDeque
    queue = deque([])  # Ok


def f():
    class FakeSet:
        pass

    set = FakeSet
    queue = deque(set())  # Ok


def f():
    queue = deque([1, 2])  # Ok


def f():
    queue = deque([1, 2], maxlen=10)  # Ok


def f():
    queue = deque()  # Ok
