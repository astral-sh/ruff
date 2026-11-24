# `pytest-parametrize-names-wrong-type` (`PT006`)

```toml
[lint]
select = ["PT006"]
```

## Avoid fixes for unknown `argvalues`

The following is a regression test for [#24715]. It's unsafe to replace `("param",)` with just
`"param"` because the elements of the list may be tuples themselves. We still emit a diagnostic, but
the fix should be suppressed.

```py
import pytest

variable = (2,)

@pytest.mark.parametrize(("param",), [(1,), variable])  # snapshot: pytest-parametrize-names-wrong-type
def test_single_element_tuple_and_variable_mix(param):
    ...
```

```snapshot
error[PT006]: Wrong type passed to first argument of `pytest.mark.parametrize`; expected `str`
 --> src/mdtest_snippet.py:5:26
  |
5 | @pytest.mark.parametrize(("param",), [(1,), variable])  # snapshot: pytest-parametrize-names-wrong-type
  |                          ^^^^^^^^^^
help: Use a string for the first argument
```

[#24715]: https://github.com/astral-sh/ruff/issues/24715
