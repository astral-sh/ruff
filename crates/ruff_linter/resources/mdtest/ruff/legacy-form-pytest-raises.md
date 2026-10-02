# `legacy-form-pytest-raises` (`RUF061`)

```toml
lint.select = ["RUF061"]
```

An extra positional argument is invalid in the context-manager form, but recommending that form is
unhelpful when the call is already a `with` context expression.

## `pytest.raises`

Legacy calls in the body are still reported.

```py
import pytest


def func(a, b):
    return a / b


def test_error_nested_in_with():
    with pytest.raises(ValueError, "oops"):  # no diagnostic
        pytest.raises(ZeroDivisionError, func, 1, b=0)  # error: [legacy-form-pytest-raises]
```

## `pytest.warns`

```py
import pytest


def test_ok_positional_args():
    with pytest.warns(UserWarning, "oops"):  # no diagnostic
        pass
```

## `pytest.deprecated_call`

```py
import pytest


def test_ok_positional_args():
    with pytest.deprecated_call("oops"):  # no diagnostic
        pass
```
