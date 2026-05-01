# pandas-stubs

```toml
[environment]
python-version = "3.14"
python-platform = "linux"

[project]
dependencies = ["pandas-stubs==3.0.0.260204"]
```

## Index arithmetic with Python sequences

`pandas-stubs` uses a structural-receiver arithmetic overload before a reverse-operation fallback.
The structural overload must not mask the fallback when an integer index is combined with a
floating-point sequence.

```py
from typing import assert_type

import pandas as pd

left = pd.Index([1, 2, 3])
floating_values = [1.0, 2.0, 3.0]
complex_values = [1j, 2j, 3j]

assert_type(left + floating_values, "pd.Index[float]")
assert_type(left + complex_values, "pd.Index[complex]")
assert_type(floating_values + left, "pd.Index[float]")
assert_type(complex_values + left, "pd.Index[complex]")
```

## Unparameterized pandas containers preserve their container type

These are representative cases from `pandas-stubs`, where arithmetic through a structural-receiver
overload should retain an unparameterized pandas container instead of producing `Unknown`.

```py
from datetime import timedelta
from typing import assert_type

import pandas as pd

def index_operations(index: pd.Index) -> None:
    bool_index = pd.Index([True, False, True])
    float_index = pd.Index([1.0, 2.0, 3.0])
    complex_index = pd.Index([1.1j, 2.2j, 4.1j])
    duration = timedelta(seconds=1)
    durations = [timedelta(seconds=value) for value in range(3)]

    assert_type(duration // index, pd.TimedeltaIndex)
    assert_type(durations // index, pd.Index)
    assert_type(index * bool_index, pd.Index)
    assert_type(index * float_index, pd.Index)
    assert_type(index * complex_index, pd.Index)

def series_operations(series: pd.Series) -> None:
    bool_index = pd.Index([True, False, True])
    complex_index = pd.Index([1.1j, 2.2j, 4.1j])
    bool_series = pd.Series([True, False, True])
    complex_series = pd.Series([1.1j, 2.2j, 4.1j])
    complex_values = [1j, 1j, 4j]
    duration = timedelta(seconds=1)
    durations = [timedelta(seconds=value) for value in range(3)]

    assert_type(complex_values + series, pd.Series)
    assert_type(duration // series, "pd.Series[pd.Timedelta]")
    assert_type(durations // series, pd.Series)
    assert_type(series.rfloordiv(durations), "pd.Series[pd.Timedelta]")
    assert_type(series * bool_index, pd.Series)
    assert_type(series * complex_index, pd.Series)
    assert_type(series.mul(bool_index), pd.Series)
    assert_type(series.mul(complex_index), pd.Series)
    assert_type(series.rmul(bool_index), pd.Series)
    assert_type(series.rmul(complex_index), pd.Series)
    assert_type(series * bool_series, pd.Series)
    assert_type(series * complex_series, pd.Series)
    assert_type(series.mul(bool_series), pd.Series)
    assert_type(series.mul(complex_series), pd.Series)
    assert_type(series.rmul(bool_series), pd.Series)
    assert_type(series.rmul(complex_series), pd.Series)
```
