def simple():
    from math import sqrt
    return sqrt(9)


def aliased_members():
    from math import sqrt as root, ceil, pi as PI
    return root(9), ceil(PI)


def closure_and_shadowing():
    from math import sqrt

    def closure():
        return sqrt(16)

    def shadow(sqrt):
        return sqrt(25)

    return closure, shadow


def comprehensions():
    from math import sqrt
    return [sqrt(n) for n in (4, 9)], [sqrt for sqrt in (1, 2)]


def default_argument():
    from math import sqrt

    def inner(value=sqrt(9)):
        return value

    return inner


def configured_alias():
    from pandas import Series as S, DataFrame
    return S(), DataFrame()


def dotted_module():
    from logging.config import dictConfig as configure
    configure({})


def reuse_alias():
    import math as m
    from math import sqrt
    return sqrt(9), m.pi


def reuse_dotted_module():
    import logging.config
    from logging.config import dictConfig
    dictConfig({})


def repeated_imports():
    from math import sqrt
    from math import ceil
    return sqrt(9), ceil(1.1)


def parenthesized():
    from math import (
        sqrt,
        ceil,
    )
    return sqrt(9), ceil(1.1)


def trailing_comment():
    from math import sqrt  # Keep this comment.
    return sqrt(9)


def semicolon():
    from math import sqrt; result = sqrt(9)
    return result


def unused():
    from math import sqrt


def annotations():
    from typing import Optional, Union
    value: Optional[Union[int, str]] = None
    return value


def type_checking():
    import typing
    if typing.TYPE_CHECKING:
        from pandas import Series
        value: Series


def attribute_and_f_string():
    from math import sqrt
    return sqrt.__name__, f"{sqrt(9)}"


def suppressed_import():
    from math import sqrt  # noqa: ICN003
    from math import ceil
    return sqrt(9), ceil(1.1)


def reuse_semicolon():
    import math; from math import sqrt; print(sqrt(9))


def same_branch():
    if condition:
        import math
        from math import sqrt
        print(sqrt(9))