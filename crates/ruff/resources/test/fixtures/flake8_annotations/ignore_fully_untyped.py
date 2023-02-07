"""Test case expected to be run with `ignore_fully_untyped = True`."""


def ok_fully_untyped_1(a, b):
    pass


def ok_fully_untyped_2():
    pass


def ok_fully_typed_1(a: int, b: int) -> int:
    pass


def ok_fully_typed_2() -> int:
    pass


def ok_fully_typed_3(a: int, *args: str, **kwargs: str) -> int:
    pass


def error_partially_typed_1(a: int, b):
    pass


def error_partially_typed_2(a: int, b) -> int:
    pass


def error_partially_typed_3(a: int, b: int):
    pass


class X:
    def ok_untyped_method_with_arg(self, a):
        pass

    def ok_untyped_method(self):
        pass

    def error_typed_self(self: X):
        pass
