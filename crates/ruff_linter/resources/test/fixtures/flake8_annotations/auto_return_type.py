def func():
    return 1


def func():
    return 1.5


def func(x: int):
    if x > 0:
        return 1
    else:
        return 1.5


def func():
    return True


def func(x: int):
    if x > 0:
        return None
    else:
        return


def func(x: int):
    return 1 or 2.5 if x > 0 else 1.5 or "str"


def func(x: int):
    return 1 + 2.5 if x > 0 else 1.5 or "str"


def func(x: int):
    if not x:
        return None
    return {"foo": 1}


def func(x: int):
    return {"foo": 1}


def func(x: int):
    if not x:
        return 1
    else:
        return True


def func(x: int):
    if not x:
        return 1
    else:
        return None


def func(x: int):
    if not x:
        return 1
    elif x > 5:
        return "str"
    else:
        return None


def func(x: int):
    if x:
        return 1


def func():
    x = 1


def func(x: int):
    if x > 0:
        return 1


def func(x: int):
    match x:
        case [1, 2, 3]:
            return 1
        case 4 as y:
            return "foo"


def func(x: int):
    for i in range(5):
        if i > 0:
            return 1


def func(x: int):
    for i in range(5):
        if i > 0:
            return 1
    else:
        return 4


def func(x: int):
    for i in range(5):
        if i > 0:
            break
    else:
        return 4


def func(x: int):
    try:
        pass
    except:
        return 1


def func(x: int):
    try:
        pass
    except:
        return 1
    finally:
        return 2


def func(x: int):
    try:
        pass
    except:
        return 1
    else:
        return 2


def func(x: int):
    try:
        return 1
    except:
        return 2
    else:
        pass


def func(x: int):
    while x > 0:
        break
        return 1


import abc
from abc import abstractmethod


class Foo(abc.ABC):
    @abstractmethod
    def method(self):
        pass

    @abc.abstractmethod
    def method(self):
        """Docstring."""

    @abc.abstractmethod
    def method(self):
        ...

    @staticmethod
    @abstractmethod
    def method():
        pass

    @classmethod
    @abstractmethod
    def method(cls):
        pass

    @abstractmethod
    def method(self):
        if self.x > 0:
            return 1
        else:
            return 1.5


def func(x: int):
    try:
        pass
    except:
        return 2


def func(x: int):
    try:
        pass
    except:
        return 2
    else:
        return 3


def func(x: int):
    if not x:
        raise ValueError
    else:
        raise TypeError


def func(x: int):
    if not x:
        raise ValueError
    else:
        return 1


from typing import overload


@overload
def overloaded(i: int) -> "int":
    ...


@overload
def overloaded(i: "str") -> "str":
    ...


def overloaded(i):
    return i


def func(x: int):
    if not x:
        return 1
    raise ValueError


def func(x: int):
    if not x:
        return 1
    else:
        return 2
    raise ValueError


def func():
    try:
        raise ValueError
    except:
        return 2


def func():
    try:
        return 1
    except:
        pass


def func(x: int):
    for _ in range(3):
        if x > 0:
            return 1
    raise ValueError


def func(x: int):
    if x > 5:
        raise ValueError
    else:
        pass


def func(x: int):
    if x > 5:
        raise ValueError
    elif x > 10:
        pass


def func(x: int):
    if x > 5:
        raise ValueError
    elif x > 10:
        return 5


def func():
    try:
        return 5
    except:
        pass

    raise ValueError


def func(x: int):
    match x:
        case [1, 2, 3]:
            return 1
        case y:
            return "foo"
