# Violations of PYI013

class OneAttributeClass:
    value: int
    ...  # Error

class OneAttributeClass2:
    ...  # Error
    value: int

class MyClass:
    ...
    value: int

class TwoEllipsesClass:
    ...
    ...  # Error

class DocstringClass:
    """
    My body only contains an ellipsis.
    """

    ...  # Error

class NonEmptyChild(Exception):
    value: int
    ...  # Error

class NonEmptyChild2(Exception):
    ...  # Error
    value: int

class NonEmptyWithInit:
    value: int
    ...  # Error

    def __init__():
        pass

# Not violations

class EmptyClass: ...
class EmptyEllipsis: ...

class Dog:
    eyes: int = 2

class WithInit:
    value: int = 0

    def __init__(): ...

def function(): ...

...
