def bar():
    ...  # OK


def foo():
    pass  # OK, since we're not in a stub file


class Bar:
    ...  # OK


class Foo:
    pass  # OK, since we're not in a stub file
