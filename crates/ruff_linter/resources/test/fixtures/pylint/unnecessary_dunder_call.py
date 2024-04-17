from typing import Any

a = 2
print((3.0).__add__(4.0))  # PLC2801
print((3.0).__sub__(4.0))  # PLC2801
print((3.0).__mul__(4.0))  # PLC2801
print((3.0).__truediv__(4.0))  # PLC2801
print((3.0).__floordiv__(4.0))  # PLC2801
print((3.0).__mod__(4.0))  # PLC2801
print((3.0).__eq__(4.0))  # PLC2801
print((3.0).__ne__(4.0))  # PLC2801
print((3.0).__lt__(4.0))  # PLC2801
print((3.0).__le__(4.0))  # PLC2801
print((3.0).__gt__(4.0))  # PLC2801
print((3.0).__ge__(4.0))  # PLC2801
print((3.0).__str__())  # PLC2801
print((3.0).__repr__())  # PLC2801
print([1, 2, 3].__len__())  # PLC2801
print((1).__neg__())  # PLC2801
print(-a.__sub__(1))  # PLC2801
print(-(a).__sub__(1))  # PLC2801
print(-(-a.__sub__(1)))  # PLC2801
print((5 - a).__sub__(1))  # PLC2801
print(-(5 - a).__sub__(1))  # PLC2801
print(-(-5 - a).__sub__(1))  # PLC2801
print(+-+-+-a.__sub__(1))  # PLC2801
print(a.__rsub__(2 - 1))  # PLC2801
print(a.__sub__(((((1))))))  # PLC2801
print(a.__sub__(((((2 - 1))))))  # PLC2801
print(a.__sub__(
    3
    +
    4
))
print(a.__rsub__(
    3
    +
    4
))
print(2 * a.__add__(3))  # PLC2801
x = 2 * a.__add__(3)  # PLC2801
x = 2 * -a.__add__(3)  # PLC2801
x = a.__add__(3)  # PLC2801
x = -a.__add__(3)  # PLC2801
x = (-a).__add__(3)  # PLC2801
x = -(-a).__add__(3)  # PLC2801

# Calls
print(a.__call__())  # PLC2801 (no fix, intentional)

# Lambda expressions
blah = lambda: a.__add__(1)  # PLC2801

# If expressions
print(a.__add__(1) if a > 0 else a.__sub__(1))  # PLC2801

# Dict/Set/List/Tuple
print({"a": a.__add__(1)})  # PLC2801
print({a.__add__(1)})  # PLC2801
print([a.__add__(1)])  # PLC2801
print((a.__add__(1),))  # PLC2801

# Comprehension variants
print({i: i.__add__(1) for i in range(5)})  # PLC2801
print({i.__add__(1) for i in range(5)})  # PLC2801
print([i.__add__(1) for i in range(5)])  # PLC2801
print((i.__add__(1) for i in range(5)))  # PLC2801

# Generators
gen = (i.__add__(1) for i in range(5))  # PLC2801
print(next(gen))

# Subscripts
print({"a": a.__add__(1)}["a"])  # PLC2801

# Starred
print(*[a.__add__(1)])  # PLC2801

# Slices
print([a.__add__(1), a.__sub__(1)][0:1])  # PLC2801


class Thing:
    def __init__(self, stuff: Any) -> None:
        super().__init__()  # OK
        super().__class__(stuff=(1, 2, 3))  # OK

    def __getattribute__(self, item):
        return object.__getattribute__(self, item)  # OK

    def do_thing(self, item):
        return object.__getattribute__(self, item)  # PLC2801

    def use_descriptor(self, item):
        item.__get__(self, type(self))  # OK
        item.__set__(self, 1)  # OK
        item.__delete__(self)  # OK


blah = lambda: {"a": 1}.__delitem__("a")  # OK

blah = dict[{"a": 1}.__delitem__("a")]  # OK
