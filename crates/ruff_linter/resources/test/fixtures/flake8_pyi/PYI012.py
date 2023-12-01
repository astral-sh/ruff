# Violations of PYI012


class OneAttributeClass:
    value: int
    pass  # PYI012 Class body must not contain `pass`


class OneAttributeClassRev:
    pass  # PYI012 Class body must not contain `pass`
    value: int


class DocstringClass:
    """
    My body only contains pass.
    """

    pass  # PYI012 Class body must not contain `pass`


class NonEmptyChild(Exception):
    value: int
    pass  # PYI012 Class body must not contain `pass`


class NonEmptyChild2(Exception):
    pass  # PYI012 Class body must not contain `pass`
    value: int


class NonEmptyWithInit:
    value: int
    pass  # PYI012 Class body must not contain `pass`

    def __init__():
        pass


# Not violations (of PYI012)


class EmptyClass:
    pass  # Y009 Empty body should contain `...`, not `pass`


class EmptyOneLine:
    pass  # Y009 Empty body should contain `...`, not `pass`


class Dog:
    eyes: int = 2


class EmptyEllipsis:
    ...


class NonEmptyEllipsis:
    value: int
    ...  # Y013 Non-empty class body must not contain `...`


class WithInit:
    value: int = 0

    def __init__():
        pass


def function():
    pass


pass
