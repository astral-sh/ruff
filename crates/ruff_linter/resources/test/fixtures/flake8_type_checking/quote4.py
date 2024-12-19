def f():
    from pandas import DataFrame

    def baz() -> DataFrame[int]:
        ...


def f():
    from typing import TYPE_CHECKING

    if TYPE_CHECKING:
        from pandas import DataFrame

    def baz() -> DataFrame[int]:
        ...


def f():
    from typing import TypeAlias, TYPE_CHECKING

    if TYPE_CHECKING:
        from pandas import DataFrame

    x: TypeAlias = DataFrame | None


def f():
    from typing import TypeAlias

    from pandas import DataFrame

    x: TypeAlias = DataFrame | None


def f():
    from typing import cast, TYPE_CHECKING

    from .foo import get_foo

    if TYPE_CHECKING:
        from pandas import DataFrame

    foo = cast(DataFrame, get_foo())


def f():
    from typing import cast

    from pandas import DataFrame

    from .foo import get_foo

    foo = cast(DataFrame, get_foo())


def f():
    from typing import TypeAlias, TYPE_CHECKING

    if TYPE_CHECKING:
        from pandas import DataFrame

    x: TypeAlias = DataFrame | dict

    assert isinstance({}, x)  # runtime use of type alias


def f():
    from typing import TypeAlias

    from pandas import DataFrame

    x: TypeAlias = DataFrame | dict

    assert isinstance({}, x)  # runtime use of type alias


def f():
    from typing import Annotated, TypeAlias
    from expensive import Foo
    from expensive2 import Foo2

    Y: TypeAlias = Annotated[Foo, "metadata"]
    some_object = Y()

    class Z(Annotated[Foo2, "metadata"]): ...
