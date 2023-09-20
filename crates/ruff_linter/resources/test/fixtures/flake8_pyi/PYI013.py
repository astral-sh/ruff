class OneAttributeClass:
    value: int
    ...


class OneAttributeClass2:
    ...
    value: int


class TwoEllipsesClass:
    ...
    ...


class DocstringClass:
    """
    My body only contains an ellipsis.
    """

    ...


class NonEmptyChild(Exception):
    value: int
    ...


class NonEmptyChild2(Exception):
    ...
    value: int


class NonEmptyWithInit:
    value: int
    ...

    def __init__():
        pass


class EmptyClass:
    ...


class EmptyEllipsis:
    ...


class Dog:
    eyes: int = 2


class WithInit:
    value: int = 0

    def __init__():
        ...


def function():
    ...


...
