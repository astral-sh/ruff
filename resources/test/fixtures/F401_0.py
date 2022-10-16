from __future__ import all_feature_names
import functools, os
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
from typing import (
    TYPE_CHECKING,
    NamedTuple,
    Dict,
    Type,
    TypeVar,
    List,
    Set,
    Union,
    cast,
)
from dataclasses import MISSING, field

from blah import ClassA, ClassB, ClassC

if TYPE_CHECKING:
    from models import Fruit, Nut, Vegetable


if TYPE_CHECKING:
    import shelve
    import importlib

if TYPE_CHECKING:
    """Hello, world!"""
    import pathlib

    z = 1


class X:
    datetime: datetime
    foo: Type["NamedTuple"]

    def a(self) -> "namedtuple":
        x = os.environ["1"]
        y = Counter()
        z = multiprocessing.pool.ThreadPool()

    def b(self) -> None:
        import pickle


__all__ = ["ClassA"] + ["ClassB"]
__all__ += ["ClassC"]

X = TypeVar("X")
Y = TypeVar("Y", bound="Dict")
Z = TypeVar("Z", "List", "Set")

a = list["Fruit"]
b = Union["""Nut""", None]
c = cast("Vegetable", b)

Field = lambda default=MISSING: field(default=default)


# Test: access a sub-importation via an alias.
import pyarrow as pa
import pyarrow.csv

print(pa.csv.read_csv("test.csv"))


# Test: referencing an import via TypeAlias.
import sys

import numpy as np

if sys.version_info >= (3, 10):
    from typing import TypeAlias
else:
    from typing_extensions import TypeAlias


CustomInt: TypeAlias = "np.int8 | np.int16"
