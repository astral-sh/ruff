"""Test case expected to be run with `mypy_init_return = True`."""

# Error
class Foo:
    def __init__(self):
        ...


# Error
class Foo:
    def __init__(self, foo):
        ...


# OK
class Foo:
    def __init__(self, foo) -> None:
        ...


# OK
class Foo:
    def __init__(self) -> None:
        ...


# OK
class Foo:
    def __init__(self, foo: int):
        ...


# OK
class Foo:
    def __init__(self, foo: int) -> None:
        ...


# Error
def __init__(self, foo: int):
    ...
