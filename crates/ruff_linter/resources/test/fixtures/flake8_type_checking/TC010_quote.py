from typing import TypeVar, NewType


a = TypeVar('a', int | 'str', str)

b = TypeVar('b', 'int' | 'str', bool)

c = TypeVar('c', bound="int" | str)

d = NewType('d', "int" | str)

e = TypeVar('e', bound='int' | 'str' | 'float')

f = TypeVar('f', "Foo['Bar']" | None, str)

g = TypeVar('g', 'Dict["Key", "Value"]' | None, dict)

h = TypeVar('h', "list[int]" | 'dict[str, int]', list)

i = TypeVar('i', bound="tuple[int, ...]" | 'set[str]')

j = TypeVar('j', """int""" | str, float)

k = TypeVar('k', '''str''' | bool, int)

l = TypeVar('l', "Literal['a\"b']" | None, str)
