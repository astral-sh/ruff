from typing import Any


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


class Thing:
    def __init__(self, stuff: Any) -> None:
        super().__init__()  # OK
        super().__class__(stuff=(1, 2, 3))  # OK

    def __getattribute__(self, item):
        return object.__getattribute__(self, item)  # OK

    def do_thing(self, item):
        return object.__getattribute__(self, item)  # PLC2801


blah = lambda: {"a": 1}.__delitem__("a")  # OK

blah = dict[{"a": 1}.__delitem__("a")]  # OK
