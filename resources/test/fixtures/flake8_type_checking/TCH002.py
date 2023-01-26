"""Tests to determine accurate detection of typing-only imports."""


def f():
    import pandas as pd  # TCH002

    x: pd.DataFrame


def f():
    from pandas import DataFrame  # TCH002

    x: DataFrame


def f():
    from pandas import DataFrame as df  # TCH002

    x: df


def f():
    import pandas as pd  # TCH002

    x: pd.DataFrame = 1


def f():
    from pandas import DataFrame  # TCH002

    x: DataFrame = 2


def f():
    from pandas import DataFrame as df  # TCH002

    x: df = 3


def f():
    import pandas as pd  # TCH002

    x: "pd.DataFrame" = 1


def f():
    import pandas as pd

    x = dict["pd.DataFrame", "pd.DataFrame"]  # TCH002


def f():
    import pandas as pd

    print(pd)


def f():
    from pandas import DataFrame

    print(DataFrame)


def f():
    from pandas import DataFrame

    def f():
        print(DataFrame)


def f():
    from typing import Dict, Any

    def example() -> Any:
        return 1

    x: Dict[int] = 20


def f():
    from typing import TYPE_CHECKING

    if TYPE_CHECKING:
        from typing import Dict
    x: Dict[int] = 20


def f():
    from pathlib import Path

    class ImportVisitor(ast.NodeTransformer):
        def __init__(self, cwd: Path) -> None:
            self.cwd = cwd
            origin = Path(spec.origin)

    class ExampleClass:
        def __init__(self):
            self.cwd = Path(pandas.getcwd())


def f():
    import pandas

    class Migration:
        enum = pandas


def f():
    import pandas

    class Migration:
        enum = pandas.EnumClass


def f():
    from typing import TYPE_CHECKING

    from pandas import y

    if TYPE_CHECKING:
        _type = x
    else:
        _type = y


def f():
    from typing import TYPE_CHECKING

    from pandas import y

    if TYPE_CHECKING:
        _type = x
    elif True:
        _type = y


def f():
    from typing import cast

    import pandas as pd

    x = cast(pd.DataFrame, 2)


def f():
    import pandas as pd

    x = dict[pd.DataFrame, pd.DataFrame]
