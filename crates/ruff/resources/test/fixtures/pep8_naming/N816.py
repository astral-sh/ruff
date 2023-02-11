import collections
from collections import namedtuple
from typing import NamedTuple, TypedDict

lower = 0
CONSTANT = 0
mixedCase = 0
_mixedCase = 0
mixed_Case = 0
myObj1 = collections.namedtuple("MyObj1", ["a", "b"])
myObj2 = namedtuple("MyObj2", ["a", "b"])
Employee = NamedTuple('Employee', [('name', str), ('id', int)])
Point2D = TypedDict('Point2D', {'in': int, 'x-y': int})
