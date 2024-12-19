### Errors

class Person:
    def __init__(self):
        self.name = "monty"

    def __eq__(self, other):
        return isinstance(other, Person) and other.name == self.name


class MaybeEqIf:
    if ...:
        def __eq__(self, other): ...


class MaybeEqElif:
    if ...:
        ...
    elif ...:
        def __eq__(self, other): ...


class MaybeEqElse:
    if ...:
        ...
    else:
        def __eq__(self, other): ...


class MaybeEqWith:
    with ...:
        def __eq__(self, other): ...


class MaybeEqFor:
    for _ in ...:
        def __eq__(self, other): ...


class MaybeEqForElse:
    for _ in ...:
        ...
    else:
        def __eq__(self, other): ...


class MaybeEqWhile:
    while ...:
        def __eq__(self, other): ...


class MaybeEqWhileElse:
    while ...:
        ...
    else:
        def __eq__(self, other): ...


class MaybeEqTry:
    try:
        def __eq__(self, other): ...
    except Exception:
        ...


class MaybeEqTryExcept:
    try:
        ...
    except Exception:
        def __eq__(self, other): ...


class MaybeEqTryExceptElse:
    try:
        ...
    except Exception:
        ...
    else:
        def __eq__(self, other): ...


class MaybeEqTryFinally:
    try:
        ...
    finally:
        def __eq__(self, other): ...


class MaybeEqMatchCase:
    match ...:
        case int():
            def __eq__(self, other): ...


class MaybeEqMatchCaseWildcard:
    match ...:
        case int(): ...
        case _:
            def __eq__(self, other): ...


class MaybeEqDeeplyNested:
    if ...:
        ...
    else:
        with ...:
            for _ in ...:
                def __eq__(self, other): ...


### OK

class Language:
    def __init__(self):
        self.name = "python"

    def __eq__(self, other):
        return isinstance(other, Language) and other.name == self.name

    def __hash__(self):
        return hash(self.name)


class MyClass:
    def __eq__(self, other):
        return True

    __hash__ = None


class SingleClass:
    def __eq__(self, other):
        return True

    def __hash__(self):
        return 7


class ChildClass(SingleClass):
    def __eq__(self, other):
        return True

    __hash__ = SingleClass.__hash__
