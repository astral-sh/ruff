from __future__ import all_feature_names
import os
import functools
from collections import (
    Counter,
    OrderedDict,
    namedtuple,
)
import multiprocessing.pool
import multiprocessing.process
import logging.config
import logging.handlers

from blah import ClassA, ClassB, ClassC


class X:
    def a(self) -> "namedtuple":
        x = os.environ["1"]
        y = Counter()
        z = multiprocessing.pool.ThreadPool()


__all__ = ["ClassA"] + ["ClassB"]
__all__ += ["ClassC"]
