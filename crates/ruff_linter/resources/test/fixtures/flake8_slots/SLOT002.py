from collections import namedtuple
from typing import NamedTuple


class Bad(namedtuple("foo", ["str", "int"])):  # SLOT002
    pass


class Good(namedtuple("foo", ["str", "int"])):  # OK
    __slots__ = ("foo",)


class Good(NamedTuple):  # Ok
    pass
