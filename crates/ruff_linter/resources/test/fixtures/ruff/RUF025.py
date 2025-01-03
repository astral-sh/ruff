from collections import deque
import collections


def f():
    queue = collections.deque([])  # RUF025


def f():
    queue = collections.deque([], maxlen=10)  # RUF025


def f():
    queue = deque([])  # RUF025


def f():
    queue = deque(())  # RUF025


def f():
    queue = deque({})  # RUF025


def f():
    queue = deque(set())  # RUF025


def f():
    queue = collections.deque([], maxlen=10)  # RUF025


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
