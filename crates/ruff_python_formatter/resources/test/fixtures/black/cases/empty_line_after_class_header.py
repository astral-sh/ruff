class Foo:

    x = 1


class Bar:

    # leading comment should still strip the blank line
    y = 2


class Baz:
    # no blank line, leave as-is
    z = 3


class Qux:
    """docstring takes precedence over the blank-line rule."""

    a = 1