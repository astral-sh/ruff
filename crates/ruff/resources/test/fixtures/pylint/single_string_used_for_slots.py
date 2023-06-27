# Errors.


class Foo:
    __slots__ = "bar"

    def __init__(self, bar):
        self.bar = bar


# Non-errors.


class Foo:
    __slots__ = ("bar",)

    def __init__(self, bar):
        self.bar = bar
