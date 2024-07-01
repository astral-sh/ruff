from contextvars import ContextVar
from types import MappingProxyType
import re
import collections
import time

# Okay
ContextVar("cv")
ContextVar("cv", default=())
ContextVar("cv", default=(1, 2, 3))
ContextVar("cv", default="foo")
ContextVar("cv", default=tuple())
ContextVar("cv", default=frozenset())
ContextVar("cv", default=MappingProxyType({}))
ContextVar("cv", default=re.compile("foo"))
ContextVar("cv", default=float(1))

# Bad
ContextVar("cv", default=[])
ContextVar("cv", default={})
ContextVar("cv", default=list())
ContextVar("cv", default=set())
ContextVar("cv", default=dict())
ContextVar("cv", default=[char for char in "foo"])
ContextVar("cv", default={char for char in "foo"})
ContextVar("cv", default={char: idx for idx, char in enumerate("foo")})
ContextVar("cv", default=collections.deque())

def bar() -> list[int]:
    return [1, 2, 3]

ContextVar("cv", default=bar())
ContextVar("cv", default=time.time())

def baz(): ...
ContextVar("cv", default=baz())
