from typing import TypeVar, NewType


a = TypeVar('a', int | 'str', str)

b = TypeVar('b', 'int' | 'str', bool)

c = TypeVar('c', bound="int" | str)

d = NewType('d', "int" | str)
