def func(_, a, A):
    return _, a, A


class Class:
    def method(self, _, a, A):
        return _, a, A


def func(_, setUp):
    return _, setUp


from typing import override

class Extended(Class):
    @override
    def method(self, _, a, A): ...


@override  # Incorrect usage
def func(_, a, A): ...


func = lambda _, a, A: ...


class Extended(Class):
    method = override(lambda self, _, a, A: ...)  # Incorrect usage
