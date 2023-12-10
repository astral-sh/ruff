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
