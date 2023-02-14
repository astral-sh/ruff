import collections
from collections import namedtuple
from typing import TypeVar
from typing import NewType
from typing import NamedTuple, TypedDict

GLOBAL: str = "foo"


def assign():
    global GLOBAL
    GLOBAL = "bar"
    lower = 0
    Camel = 0
    CONSTANT = 0
    _ = 0

    MyObj1 = collections.namedtuple("MyObj1", ["a", "b"])
    MyObj2 = namedtuple("MyObj12", ["a", "b"])

    T = TypeVar("T")
    UserId = NewType("UserId", int)

    Employee = NamedTuple('Employee', [('name', str), ('id', int)])

    Point2D = TypedDict('Point2D', {'in': int, 'x-y': int})


def aug_assign(rank, world_size):
    global CURRENT_PORT

    CURRENT_PORT += 1
    if CURRENT_PORT > MAX_PORT:
        CURRENT_PORT = START_PORT


def loop_assign():
    global CURRENT_PORT
    for CURRENT_PORT in range(5):
        pass
