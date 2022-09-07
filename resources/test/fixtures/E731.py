from typing import Callable, Iterable

a = lambda x: x**2
b = map(lambda x: 2 * x, range(3))
c: Callable = lambda x: x**2
d: Iterable = map(lambda x: 2 * x, range(3))
