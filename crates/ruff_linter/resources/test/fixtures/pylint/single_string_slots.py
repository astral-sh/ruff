# Errors.
class Foo:
    __slots__ = "bar"

    def __init__(self, bar):
        self.bar = bar


class Foo:
    __slots__: str = "bar"

    def __init__(self, bar):
        self.bar = bar


class Foo:
    __slots__: str = f"bar"

    def __init__(self, bar):
        self.bar = bar


# Non-errors.
class Foo:
    __slots__ = ("bar",)

    def __init__(self, bar):
        self.bar = bar


class Foo:
    __slots__: tuple[str, ...] = ("bar",)

    def __init__(self, bar):
        self.bar = bar

# This is a type error, out of scope for the rule
class Foo:
    __slots__ = t"bar{baz}"

    def __init__(self, bar):
        self.bar = bar

