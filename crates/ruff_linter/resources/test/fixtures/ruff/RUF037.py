import collections
from collections import deque


def f():
    queue = collections.deque([])  # RUF037


def f():
    queue = collections.deque([], maxlen=10)  # RUF037


def f():
    queue = deque([])  # RUF037


def f():
    queue = deque(())  # RUF037


def f():
    queue = deque({})  # RUF037


def f():
    queue = deque(set())  # RUF037


def f():
    queue = collections.deque([], maxlen=10)  # RUF037


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

def f():
    x = 0 or(deque)([])


# regression tests for https://github.com/astral-sh/ruff/issues/18612
def f():
    deque([], *[10])  # RUF037 but no fix
    deque([], **{"maxlen": 10})  # RUF037
    deque([], foo=1)  # RUF037


# Somewhat related to the issue, both okay because we can't generally look
# inside *args or **kwargs
def f():
    deque(*([], 10))  # Ok
    deque(**{"iterable": [], "maxlen": 10})  # Ok

# The fix was actually always unsafe in the presence of comments. all of these
# are deleted
def f():
    deque(  # a comment in deque, deleted
        [  # a comment _in_ the list, deleted
        ],  # a comment after the list, deleted
        maxlen=10,  # a comment on maxlen, deleted
        )  # only this is preserved


# `maxlen` can also be passed positionally
def f():
    deque([], 10)


def f():
    deque([], iterable=[])

# https://github.com/astral-sh/ruff/issues/18854
deque("")
deque(b"")
deque(f"")
deque(f"" "")
deque(f"" f"")
deque("abc") # OK
deque(b"abc") # OK
deque(f"" "a")  # OK
deque(f"{x}" "")  # OK

# https://github.com/astral-sh/ruff/issues/19951
deque(t"")
deque(t""  t"")
deque(t"{""}") # OK

# https://github.com/astral-sh/ruff/issues/20050
deque(f"{""}")  # RUF037

deque(f"{b""}")
deque(f"{""=}")
deque(f"{""!a}")
deque(f"{""!r}")
deque(f"{"":1}")
