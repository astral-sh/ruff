from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import module
    from module import Class


def f(var: Class) -> Class:
    x: Class


def f(var: module.Class) -> module.Class:
    x: module.Class


def f():
    print(Class)


def f():
    print(module.Class)
