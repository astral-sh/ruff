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
pd.arrays.PandasArray  # snapshot: pandas3-deprecation
pd.api.types.is_interval(x)  # error: [pandas3-deprecation]
pd.api.types.is_period(x)  # error: [pandas3-deprecation]
```

```snapshot
error[pandas3-deprecation]: `pd.value_counts` was removed in pandas 3.0. Use `pd.Series(...).value_counts()` instead.
 --> src/mdtest_snippet.py:3:1
  |
3 | pd.value_counts([1, 2, 2])  # snapshot: pandas3-deprecation
  | ^^^^^^^^^^^^^^^


error[pandas3-deprecation]: `pd.arrays.PandasArray` was removed in pandas 3.0. Use `pd.arrays.NumpyExtensionArray` instead.
 --> src/mdtest_snippet.py:5:1
  |
5 | pd.arrays.PandasArray  # snapshot: pandas3-deprecation
  | ^^^^^^^^^^^^^^^^^^^^^
help: Replace with `NumpyExtensionArray`
  |
4 | pd.read_gbq("SELECT 1")  # error: [pandas3-deprecation]
  - pd.arrays.PandasArray  # snapshot: pandas3-deprecation
5 + pd.arrays.NumpyExtensionArray  # snapshot: pandas3-deprecation
6 | pd.api.types.is_interval(x)  # error: [pandas3-deprecation]
  |
note: This is an unsafe fix and may change runtime behavior
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
help: Replace with `sep=r"\s+"`
  |
2 |
  - pd.read_csv("data.txt", delim_whitespace=True)  # snapshot: pandas3-deprecation
3 + pd.read_csv("data.txt", sep=r"\s+")  # snapshot: pandas3-deprecation
4 | pd.read_table("data.txt", verbose=True)  # error: [pandas3-deprecation]
  |
```

## Fixes for removed keyword arguments

```py
import pandas as pd

pd.read_csv("data.csv", verbose=True)  # snapshot: pandas3-deprecation
pd.read_csv("data.csv", infer_datetime_format=True)  # snapshot: pandas3-deprecation
pd.read_parquet("data.parquet", use_nullable_dtypes=True)  # snapshot: pandas3-deprecation
pd.read_parquet("data.parquet", use_nullable_dtypes=False)  # snapshot: pandas3-deprecation
pd.Series([1, 2], fastpath=True)  # snapshot: pandas3-deprecation
```

```snapshot
error[pandas3-deprecation]: `pd.read_csv(verbose=...)` was removed in pandas 3.0. Remove the argument.
 --> src/mdtest_snippet.py:3:25
  |
3 | pd.read_csv("data.csv", verbose=True)  # snapshot: pandas3-deprecation
  |                         ^^^^^^^^^^^^
help: Remove `verbose`
  |
2 |
  - pd.read_csv("data.csv", verbose=True)  # snapshot: pandas3-deprecation
3 + pd.read_csv("data.csv")  # snapshot: pandas3-deprecation
4 | pd.read_csv("data.csv", infer_datetime_format=True)  # snapshot: pandas3-deprecation
  |


error[pandas3-deprecation]: `pd.read_csv(infer_datetime_format=...)` was removed in pandas 3.0. Remove the argument; strict datetime format inference is now the default.
 --> src/mdtest_snippet.py:4:25
  |
4 | pd.read_csv("data.csv", infer_datetime_format=True)  # snapshot: pandas3-deprecation
  |                         ^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Remove `infer_datetime_format`
  |
3 | pd.read_csv("data.csv", verbose=True)  # snapshot: pandas3-deprecation
  - pd.read_csv("data.csv", infer_datetime_format=True)  # snapshot: pandas3-deprecation
4 + pd.read_csv("data.csv")  # snapshot: pandas3-deprecation
5 | pd.read_parquet("data.parquet", use_nullable_dtypes=True)  # snapshot: pandas3-deprecation
  |
note: This is an unsafe fix and may change runtime behavior


error[pandas3-deprecation]: `pd.read_parquet(use_nullable_dtypes=...)` was removed in pandas 3.0. Use `dtype_backend="numpy_nullable"` instead.
 --> src/mdtest_snippet.py:5:33
  |
5 | pd.read_parquet("data.parquet", use_nullable_dtypes=True)  # snapshot: pandas3-deprecation
  |                                 ^^^^^^^^^^^^^^^^^^^^^^^^
help: Replace with `dtype_backend="numpy_nullable"`
  |
4 | pd.read_csv("data.csv", infer_datetime_format=True)  # snapshot: pandas3-deprecation
  - pd.read_parquet("data.parquet", use_nullable_dtypes=True)  # snapshot: pandas3-deprecation
5 + pd.read_parquet("data.parquet", dtype_backend="numpy_nullable")  # snapshot: pandas3-deprecation
6 | pd.read_parquet("data.parquet", use_nullable_dtypes=False)  # snapshot: pandas3-deprecation
  |
note: This is an unsafe fix and may change runtime behavior


error[pandas3-deprecation]: `pd.read_parquet(use_nullable_dtypes=...)` was removed in pandas 3.0. Use `dtype_backend="numpy_nullable"` instead.
 --> src/mdtest_snippet.py:6:33
  |
6 | pd.read_parquet("data.parquet", use_nullable_dtypes=False)  # snapshot: pandas3-deprecation
  |                                 ^^^^^^^^^^^^^^^^^^^^^^^^^
help: Remove `use_nullable_dtypes`
  |
5 | pd.read_parquet("data.parquet", use_nullable_dtypes=True)  # snapshot: pandas3-deprecation
  - pd.read_parquet("data.parquet", use_nullable_dtypes=False)  # snapshot: pandas3-deprecation
6 + pd.read_parquet("data.parquet")  # snapshot: pandas3-deprecation
7 | pd.Series([1, 2], fastpath=True)  # snapshot: pandas3-deprecation
  |


error[pandas3-deprecation]: `pd.Series(fastpath=...)` was removed in pandas 3.0. Remove the argument.
 --> src/mdtest_snippet.py:7:19
  |
7 | pd.Series([1, 2], fastpath=True)  # snapshot: pandas3-deprecation
  |                   ^^^^^^^^^^^^^
help: Remove `fastpath`
  |
6 | pd.read_parquet("data.parquet", use_nullable_dtypes=False)  # snapshot: pandas3-deprecation
  - pd.Series([1, 2], fastpath=True)  # snapshot: pandas3-deprecation
7 + pd.Series([1, 2])  # snapshot: pandas3-deprecation
  |
```

## Removed argument values

```py
import pandas as pd

pd.to_datetime(values, errors="ignore")  # error: [pandas3-deprecation]
pd.to_numeric(values, errors="ignore")  # error: [pandas3-deprecation]
pd.to_timedelta(values, errors="ignore")  # error: [pandas3-deprecation]
pd.set_option("mode.use_inf_as_na", True)  # error: [pandas3-deprecation]
pd.option_context("mode.use_inf_as_na", True)  # error: [pandas3-deprecation]
pd.read_csv("data.csv", delim_whitespace=False)  # snapshot: pandas3-deprecation
pd.read_csv("data.txt", sep=";", delim_whitespace=True)  # snapshot: pandas3-deprecation
pd.StringDtype("pyarrow_numpy")  # error: [pandas3-deprecation]
pd.StringDtype(storage="pyarrow_numpy")  # error: [pandas3-deprecation]
```

```snapshot
error[pandas3-deprecation]: `pd.read_csv(delim_whitespace=...)` was removed in pandas 3.0. Remove the argument.
 --> src/mdtest_snippet.py:8:25
  |
8 | pd.read_csv("data.csv", delim_whitespace=False)  # snapshot: pandas3-deprecation
  |                         ^^^^^^^^^^^^^^^^^^^^^^


error[pandas3-deprecation]: `pd.read_csv(delim_whitespace=...)` was removed in pandas 3.0. Use `sep=r"\s+"` instead.
 --> src/mdtest_snippet.py:9:34
  |
9 | pd.read_csv("data.txt", sep=";", delim_whitespace=True)  # snapshot: pandas3-deprecation
  |                                  ^^^^^^^^^^^^^^^^^^^^^
```

## Removed frequency aliases

```py
import pandas as pd

pd.date_range("2024-01-01", periods=3, freq="M")  # snapshot: pandas3-deprecation
pd.date_range("2024-01-01", periods=3, freq="15T")  # snapshot: pandas3-deprecation
pd.date_range("2024-01-01", periods=3, freq="Q-NOV")  # snapshot: pandas3-deprecation
pd.date_range("2024-01-01", "2024-01-02", None, "H")  # error: [pandas3-deprecation]
pd.bdate_range("2024-01-01", periods=3, freq="BM")  # error: [pandas3-deprecation]
pd.timedelta_range("1 day", periods=3, freq="T")  # error: [pandas3-deprecation]
pd.date_range("2024-01-01", periods=3, freq="A-DEC")  # error: [pandas3-deprecation]
pd.date_range("2024-01-01", periods=3, freq="AS")  # error: [pandas3-deprecation]
pd.date_range("2024-01-01", periods=3, freq="Y")  # error: [pandas3-deprecation]
pd.Grouper(key="ts", freq="2H")  # error: [pandas3-deprecation]
pd.tseries.frequencies.to_offset("L")  # error: [pandas3-deprecation]
```

```snapshot
error[pandas3-deprecation]: `pd.date_range(freq="M")` was removed in pandas 3.0. Use `freq="ME"` instead.
 --> src/mdtest_snippet.py:3:45
  |
3 | pd.date_range("2024-01-01", periods=3, freq="M")  # snapshot: pandas3-deprecation
  |                                             ^^^
help: Replace with `"ME"`
  |
2 |
  - pd.date_range("2024-01-01", periods=3, freq="M")  # snapshot: pandas3-deprecation
3 + pd.date_range("2024-01-01", periods=3, freq="ME")  # snapshot: pandas3-deprecation
4 | pd.date_range("2024-01-01", periods=3, freq="15T")  # snapshot: pandas3-deprecation
  |
note: This is an unsafe fix and may change runtime behavior


error[pandas3-deprecation]: `pd.date_range(freq="15T")` was removed in pandas 3.0. Use `freq="15min"` instead.
 --> src/mdtest_snippet.py:4:45
  |
4 | pd.date_range("2024-01-01", periods=3, freq="15T")  # snapshot: pandas3-deprecation
  |                                             ^^^^^
help: Replace with `"15min"`
  |
3 | pd.date_range("2024-01-01", periods=3, freq="M")  # snapshot: pandas3-deprecation
  - pd.date_range("2024-01-01", periods=3, freq="15T")  # snapshot: pandas3-deprecation
4 + pd.date_range("2024-01-01", periods=3, freq="15min")  # snapshot: pandas3-deprecation
5 | pd.date_range("2024-01-01", periods=3, freq="Q-NOV")  # snapshot: pandas3-deprecation
  |
note: This is an unsafe fix and may change runtime behavior


error[pandas3-deprecation]: `pd.date_range(freq="Q-NOV")` was removed in pandas 3.0. Use `freq="QE-NOV"` instead.
 --> src/mdtest_snippet.py:5:45
  |
5 | pd.date_range("2024-01-01", periods=3, freq="Q-NOV")  # snapshot: pandas3-deprecation
  |                                             ^^^^^^^
help: Replace with `"QE-NOV"`
  |
4 | pd.date_range("2024-01-01", periods=3, freq="15T")  # snapshot: pandas3-deprecation
  - pd.date_range("2024-01-01", periods=3, freq="Q-NOV")  # snapshot: pandas3-deprecation
5 + pd.date_range("2024-01-01", periods=3, freq="QE-NOV")  # snapshot: pandas3-deprecation
6 | pd.date_range("2024-01-01", "2024-01-02", None, "H")  # error: [pandas3-deprecation]
  |
note: This is an unsafe fix and may change runtime behavior
```

## Removed `Timedelta` units

```py
import pandas as pd

pd.Timedelta(5, unit="H")  # snapshot: pandas3-deprecation
pd.Timedelta(5, "T")  # error: [pandas3-deprecation]
pd.to_timedelta([1, 2], unit="N")  # error: [pandas3-deprecation]
```

```snapshot
error[pandas3-deprecation]: `pd.Timedelta(unit="H")` was removed in pandas 3.0. Use `unit="h"` instead.
 --> src/mdtest_snippet.py:3:22
  |
3 | pd.Timedelta(5, unit="H")  # snapshot: pandas3-deprecation
  |                      ^^^
help: Replace with `"h"`
  |
2 |
  - pd.Timedelta(5, unit="H")  # snapshot: pandas3-deprecation
3 + pd.Timedelta(5, unit="h")  # snapshot: pandas3-deprecation
4 | pd.Timedelta(5, "T")  # error: [pandas3-deprecation]
  |
note: This is an unsafe fix and may change runtime behavior
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
pd.read_csv("data.txt", sep=",")
pd.date_range("2024-01-01", periods=3, freq="ME")
pd.date_range("2024-01-01", periods=3, freq="MS")
pd.date_range("2024-01-01", periods=3, freq="15min")
pd.date_range("2024-01-01", periods=3, freq="QE-NOV")
pd.date_range("2024-01-01", periods=3, freq="D")
pd.date_range("2024-01-01", periods=3, freq="W-SUN")
pd.Timedelta(5, unit="h")
pd.to_timedelta([1, 2], unit="ns")
```

`"M"`, `"Q"` and `"Y"` are still valid for periods, so period APIs are not flagged:

```py
import pandas as pd

pd.period_range("2024-01", periods=3, freq="M")
pd.Period("2024-01", freq="M")
pd.PeriodIndex(["2024Q1", "2024Q2"], freq="Q")
```

Frequencies that are not string literals are not checked:

```py
import pandas as pd

freq = "M"
pd.date_range("2024-01-01", periods=3, freq=freq)
```

Names that don't come from `pandas` are not flagged:

```py
def value_counts(values): ...

value_counts([1, 2, 2])
```
