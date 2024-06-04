class BazMeta(type):
    _private_count = 1

    def __new__(mcs, name, bases, attrs):
        if mcs._private_count <= 5:
            mcs.some_method()

        return super().__new__(mcs, name, bases, attrs)

    def some_method():
        pass


class Bar:
    _private = True

    @classmethod
    def is_private(cls):
        return cls._private


class Foo(metaclass=BazMeta):

    def __init__(self):
        self.public_thing = "foo"
        self._private_thing = "bar"
        self.__really_private_thing = "baz"
        self.bar = Bar()

    def __str__(self):
        return "foo"

    def get_bar():
        if self.bar._private:  # SLF001
            return None
        if self.bar()._private:  # SLF001
            return None
        if Bar._private_thing:  # SLF001
            return None
        if Foo._private_thing:
            return None
        Foo = Bar()
        if Foo._private_thing:  # SLF001
            return None
        return self.bar

    def public_func(self):
        super().public_func()

    def _private_func(self):
        super()._private_func()

    def __really_private_func(self, arg):
        super().__really_private_func(arg)

    def __eq__(self, other):
        return self._private_thing == other._private_thing


foo = Foo()

print(foo._private_thing)  # SLF001
print(foo.__really_private_thing)  # SLF001
print(foo._private_func())  # SLF001
print(foo.__really_private_func(1))  # SLF001
print(foo.bar._private)  # SLF001
print(foo()._private_thing)  # SLF001
print(foo()._private_thing__)  # SLF001

print(foo.public_thing)
print(foo.public_func())
print(foo.__dict__)
print(foo.__str__())
print(foo().__class__)
print(foo._asdict())

import os

os._exit()


from enum import Enum

Enum._missing_(1)  # OK
