# `pandas3-deprecation`

```toml
lint.preview = true
lint.select = ["pandas3-deprecation"]
```

## Removed functions and aliases

```py
import pandas as pd

pd.value_counts([1, 2, 2])  # snapshot: pandas3-deprecation
pd.read_gbq("SELECT 1")  # error: [pandas3-deprecation]
pd.arrays.PandasArray  # error: [pandas3-deprecation]
pd.api.types.is_interval(x)  # error: [pandas3-deprecation]
pd.api.types.is_period(x)  # error: [pandas3-deprecation]
```

```snapshot
error[pandas3-deprecation]: `pd.value_counts` was removed in pandas 3.0. Use `pd.Series(...).value_counts()` instead.
 --> src/mdtest_snippet.py:3:1
  |
3 | pd.value_counts([1, 2, 2])  # snapshot: pandas3-deprecation
  | ^^^^^^^^^^^^^^^
```

## Other import styles

```py
from pandas import value_counts
from pandas.api.types import is_interval
import pandas.io.sql

value_counts([1, 2, 2])  # error: [pandas3-deprecation]
is_interval(x)  # error: [pandas3-deprecation]
pandas.io.sql.execute("SELECT 1", con)  # error: [pandas3-deprecation]
```

## Removed keyword arguments

```py
import pandas as pd

pd.read_csv("data.txt", delim_whitespace=True)  # snapshot: pandas3-deprecation
pd.read_table("data.txt", verbose=True)  # error: [pandas3-deprecation]
pd.read_csv("data.csv", infer_datetime_format=True)  # error: [pandas3-deprecation]
pd.read_csv("data.csv", keep_date_col=True)  # error: [pandas3-deprecation]
pd.read_csv("data.csv", date_parser=parse)  # error: [pandas3-deprecation]
pd.read_fwf("data.txt", date_parser=parse)  # error: [pandas3-deprecation]
pd.read_excel("data.xlsx", date_parser=parse)  # error: [pandas3-deprecation]
pd.read_parquet("data.parquet", use_nullable_dtypes=True)  # error: [pandas3-deprecation]
pd.PeriodIndex(year=[2020, 2021], freq="Y")  # error: [pandas3-deprecation]
pd.PeriodIndex(ordinal=[1, 2], freq="D")  # error: [pandas3-deprecation]
pd.DatetimeIndex(dates, closed="left")  # error: [pandas3-deprecation]
pd.DatetimeIndex(dates, normalize=True)  # error: [pandas3-deprecation]
pd.TimedeltaIndex([1, 2], unit="s")  # error: [pandas3-deprecation]
pd.Series([1, 2], fastpath=True)  # error: [pandas3-deprecation]
pd.Categorical(["a"], fastpath=True)  # error: [pandas3-deprecation]
```

```snapshot
error[pandas3-deprecation]: `pd.read_csv(delim_whitespace=...)` was removed in pandas 3.0. Use `sep=r"\s+"` instead.
 --> src/mdtest_snippet.py:3:25
  |
3 | pd.read_csv("data.txt", delim_whitespace=True)  # snapshot: pandas3-deprecation
  |                         ^^^^^^^^^^^^^^^^^^^^^
```

## Removed argument values

```py
import pandas as pd

pd.to_datetime(values, errors="ignore")  # error: [pandas3-deprecation]
pd.to_numeric(values, errors="ignore")  # error: [pandas3-deprecation]
pd.to_timedelta(values, errors="ignore")  # error: [pandas3-deprecation]
pd.set_option("mode.use_inf_as_na", True)  # error: [pandas3-deprecation]
pd.StringDtype("pyarrow_numpy")  # error: [pandas3-deprecation]
pd.StringDtype(storage="pyarrow_numpy")  # error: [pandas3-deprecation]
```

## No errors

Current pandas APIs are not flagged:

```py
import pandas as pd

pd.Series([1, 2, 2]).value_counts()
pd.arrays.NumpyExtensionArray
pd.api.types.is_integer_dtype(x)
pd.read_csv("data.txt", sep=r"\s+")
pd.read_csv("data.csv", date_format="%Y-%m-%d")
pd.read_parquet("data.parquet", dtype_backend="numpy_nullable")
pd.to_datetime(values, errors="coerce")
pd.to_numeric(values, errors="raise")
pd.set_option("display.max_rows", 100)
pd.StringDtype("pyarrow")
pd.PeriodIndex.from_fields(year=[2020, 2021], quarter=[1, 2])
pd.Series([1, 2], name="x")
```

Names that don't come from `pandas` are not flagged:

```py
def value_counts(values): ...

value_counts([1, 2, 2])
```
