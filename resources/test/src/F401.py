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


class X:
    def a(self) -> "namedtuple":
        x = os.environ["1"]
        y = Counter()
        z = multiprocessing.pool.ThreadPool()
