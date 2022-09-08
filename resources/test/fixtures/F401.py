from __future__ import all_feature_names
import os
import functools
from datetime import datetime
from collections import (
    Counter,
    OrderedDict,
    namedtuple,
)
import multiprocessing.pool
import multiprocessing.process
import logging.config
import logging.handlers
from typing import NamedTuple, Dict, Type, TypeVar, List, Set

from blah import ClassA, ClassB, ClassC


class X:
    datetime: datetime
    foo: Type["NamedTuple"]

    def a(self) -> "namedtuple":
        x = os.environ["1"]
        y = Counter()
        z = multiprocessing.pool.ThreadPool()


__all__ = ["ClassA"] + ["ClassB"]
__all__ += ["ClassC"]

X = TypeVar("X")
Y = TypeVar("Y", bound="Dict")
Z = TypeVar("Z", "List", "Set")
