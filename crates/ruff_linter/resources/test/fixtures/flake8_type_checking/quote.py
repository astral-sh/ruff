def f():
    from pandas import DataFrame

    def baz() -> DataFrame:
        ...


def f():
    from pandas import DataFrame

    def baz() -> DataFrame[int]:
        ...


def f():
    from pandas import DataFrame

    def baz() -> DataFrame["int"]:
        ...


def f():
    import pandas as pd

    def baz() -> pd.DataFrame:
        ...


def f():
    import pandas as pd

    def baz() -> pd.DataFrame.Extra:
        ...


def f():
    import pandas as pd

    def baz() -> pd.DataFrame | int:
        ...



def f():
    from pandas import DataFrame

    def baz() -> DataFrame():
        ...


def f():
    from typing import Literal

    from pandas import DataFrame

    def baz() -> DataFrame[Literal["int"]]:
        ...


def f():
    from typing import TYPE_CHECKING

    if TYPE_CHECKING:
        from pandas import DataFrame

    def func(value: DataFrame):
        ...


def f():
    from pandas import DataFrame, Series

    def baz() -> DataFrame | Series:
        ...


def f():
    from pandas import DataFrame, Series

    def baz() -> (
        DataFrame |
        Series
    ):
        ...

    class C:
        x: DataFrame[
            int
        ] = 1

    def func() -> DataFrame[[DataFrame[_P, _R]], DataFrame[_P, _R]]:
        ...


def f():
    from pandas import DataFrame, Series

    def func(self) -> DataFrame | list[Series]:
        pass
