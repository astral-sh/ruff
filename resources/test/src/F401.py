import os
import functools
from collections import (
    Counter,
    OrderedDict,
    namedtuple,
)


class X:
    def a(self) -> "namedtuple":
        x = os.environ["1"]
        y = Counter()
        return X
