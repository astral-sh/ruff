# `pytest-raises-exception-group` (`ASYNC401`)

```toml
target-version = "py311"

[lint]
preview = true
select = ["ASYNC401"]
```

## Exception groups

Without additional checks, `pytest.raises` checks the group's type without
checking its contents. Use `pytest.RaisesGroup` to specify the expected
exceptions.

```py
import pytest

with pytest.raises(ExceptionGroup):  # snapshot: pytest-raises-exception-group
    raise ExceptionGroup("errors", [ValueError()])
```

```snapshot
error[ASYNC401]: Use `pytest.RaisesGroup` to specify expected exceptions in the group
 --> src/mdtest_snippet.py:3:6
  |
3 | with pytest.raises(ExceptionGroup):  # snapshot: pytest-raises-exception-group
  |      ^^^^^^^^^^^^^
```

```py
with pytest.raises(BaseExceptionGroup):  # error: [pytest-raises-exception-group]
    raise BaseExceptionGroup("errors", [KeyboardInterrupt()])

with pytest.RaisesGroup(ValueError):
    raise ExceptionGroup("errors", [ValueError()])

with pytest.RaisesGroup(KeyboardInterrupt):
    raise BaseExceptionGroup("errors", [KeyboardInterrupt()])

with pytest.raises(ValueError):
    raise ValueError()
```

## Expected exception arguments

The expected exception can be passed by position or keyword, or included in a
tuple. A call produces one diagnostic even if it contains multiple group types.

```py
import pytest

pytest.raises(ExceptionGroup)  # error: [pytest-raises-exception-group]
pytest.raises(expected_exception=BaseExceptionGroup)  # error: [pytest-raises-exception-group]
pytest.raises((ValueError, ExceptionGroup))  # error: [pytest-raises-exception-group]
pytest.raises((ExceptionGroup, BaseExceptionGroup))  # error: [pytest-raises-exception-group]
pytest.raises((ValueError, (TypeError, BaseExceptionGroup)))  # error: [pytest-raises-exception-group]
pytest.raises(expected_exception=(ExceptionGroup, ValueError))  # error: [pytest-raises-exception-group]
pytest.raises(ExceptionGroup[Exception])  # error: [pytest-raises-exception-group]
pytest.raises(expected_exception=ExceptionGroup[Exception])  # error: [pytest-raises-exception-group]
pytest.raises((ValueError, ExceptionGroup[BaseException]))  # error: [pytest-raises-exception-group]

pytest.raises(ValueError)
pytest.raises((ValueError, TypeError))
pytest.raises(expected_exception=TypeError)
pytest.raises()
pytest.raises(match="message")
```

## Single exception matchers

`pytest.RaisesExc` also checks the group's type without checking its contents.
Its expected exception argument is positional-only.

```py
import pytest

pytest.RaisesExc(ExceptionGroup)  # error: [pytest-raises-exception-group]
pytest.RaisesExc(BaseExceptionGroup)  # error: [pytest-raises-exception-group]
pytest.RaisesExc((ValueError, ExceptionGroup))  # error: [pytest-raises-exception-group]
pytest.RaisesExc(ExceptionGroup[Exception])  # error: [pytest-raises-exception-group]
pytest.RaisesExc[ExceptionGroup[Exception]](ExceptionGroup)  # error: [pytest-raises-exception-group]
pytest.RaisesExc(ExceptionGroup, check=lambda group: True)  # error: [pytest-raises-exception-group]

pytest.RaisesExc(ValueError)
pytest.RaisesExc[ValueError](ValueError)
pytest.RaisesExc((ValueError, TypeError))
pytest.RaisesExc(check=lambda exc: isinstance(exc, ExceptionGroup))
pytest.RaisesExc(expected_exception=ExceptionGroup)
```

## Nested exception groups

Passing a group type to `pytest.RaisesGroup` leaves that nested group's contents
unchecked. Every positional argument is checked, but each call produces at most
one diagnostic.

```py
import pytest

pytest.RaisesGroup(ValueError, ExceptionGroup)  # snapshot: pytest-raises-exception-group
```

```snapshot
error[ASYNC401]: Use a nested `pytest.RaisesGroup` to specify the expected exceptions
 --> src/mdtest_snippet.py:3:32
  |
3 | pytest.RaisesGroup(ValueError, ExceptionGroup)  # snapshot: pytest-raises-exception-group
  |                                ^^^^^^^^^^^^^^
```

```py
pytest.RaisesGroup(BaseExceptionGroup)  # error: [pytest-raises-exception-group]
pytest.RaisesGroup(ExceptionGroup)  # error: [pytest-raises-exception-group]
pytest.RaisesGroup(ExceptionGroup, BaseExceptionGroup)  # error: [pytest-raises-exception-group]
pytest.RaisesGroup(ExceptionGroup[Exception])  # error: [pytest-raises-exception-group]
pytest.RaisesGroup(*errors, ExceptionGroup)  # error: [pytest-raises-exception-group]

pytest.RaisesGroup(pytest.RaisesGroup(ValueError))
pytest.RaisesGroup(ValueError, pytest.RaisesGroup(TypeError))
pytest.RaisesGroup(pytest.RaisesExc(ValueError, match="message"))
pytest.RaisesGroup(ValueError, check=lambda group: isinstance(group, ExceptionGroup))
```

Type arguments on the matcher do not change which exceptions it checks.

```py
pytest.RaisesGroup[ExceptionGroup[Exception]](ExceptionGroup)  # snapshot: pytest-raises-exception-group

pytest.RaisesGroup[ValueError](ValueError)
pytest.RaisesGroup[ExceptionGroup[ValueError]](pytest.RaisesGroup(ValueError))
```

```snapshot
error[ASYNC401]: Use a nested `pytest.RaisesGroup` to specify the expected exceptions
  --> src/mdtest_snippet.py:14:47
   |
14 | pytest.RaisesGroup[ExceptionGroup[Exception]](ExceptionGroup)  # snapshot: pytest-raises-exception-group
   |                                               ^^^^^^^^^^^^^^
```

Nested calls are checked independently. A diagnostic on an inner matcher does
not also produce a diagnostic on the outer call.

```py
pytest.RaisesGroup(
    pytest.RaisesGroup(ExceptionGroup),  # error: [pytest-raises-exception-group]
)
pytest.RaisesGroup(
    pytest.RaisesExc(ExceptionGroup),  # error: [pytest-raises-exception-group]
)
```

## Expected failures

`pytest.mark.xfail(raises=ExceptionGroup)` can treat unexpected exceptions
inside a group as an expected failure. Use a `pytest.RaisesGroup` matcher to
specify the expected contents.

```py
import pytest

@pytest.mark.xfail(raises=ExceptionGroup)  # snapshot: pytest-raises-exception-group
def test_failure():
    operation()
```

```snapshot
error[ASYNC401]: Use `pytest.RaisesGroup` to specify expected exceptions in the group
 --> src/mdtest_snippet.py:3:2
  |
3 | @pytest.mark.xfail(raises=ExceptionGroup)  # snapshot: pytest-raises-exception-group
  |  ^^^^^^^^^^^^^^^^^
```

```py
pytest.mark.xfail(raises=BaseExceptionGroup)  # error: [pytest-raises-exception-group]
pytest.mark.xfail(raises=(ValueError, ExceptionGroup))  # error: [pytest-raises-exception-group]
pytest.mark.xfail(True, raises=ExceptionGroup[Exception])  # error: [pytest-raises-exception-group]
pytest.param(1, marks=pytest.mark.xfail(raises=ExceptionGroup))  # error: [pytest-raises-exception-group]

pytest.mark.xfail(raises=pytest.RaisesGroup(ValueError))
pytest.mark.xfail(raises=pytest.RaisesGroup(pytest.RaisesGroup(ValueError)))
pytest.mark.xfail(raises=ValueError)
pytest.mark.xfail(raises=(ValueError, TypeError))
```

Only the `raises` keyword specifies an expected exception type. Other arguments
and other markers are ignored.

```py
pytest.mark.xfail(ExceptionGroup)
pytest.mark.xfail(ExceptionGroup, raises=ValueError)
pytest.mark.xfail(condition=ExceptionGroup)
pytest.mark.xfail(reason="ExceptionGroup")
pytest.mark.skipif(raises=ExceptionGroup)
```

## Additional checks

`match`, `check`, and assertions after the context manager do not suppress the
diagnostic.

```py
import pytest

pytest.raises(ExceptionGroup, match="errors")  # error: [pytest-raises-exception-group]
pytest.raises(ExceptionGroup, check=lambda group: len(group.exceptions) == 1)  # error: [pytest-raises-exception-group]
pytest.raises(BaseExceptionGroup, match="errors", check=lambda group: True)  # error: [pytest-raises-exception-group]
```

## Call contexts

The rule applies in both synchronous and asynchronous functions, and to the
callable form of `pytest.raises`.

```py
import pytest

def check_group():
    with pytest.raises(ExceptionGroup):  # error: [pytest-raises-exception-group]
        operation()

async def check_async_group():
    with pytest.raises(ExceptionGroup):  # error: [pytest-raises-exception-group]
        await operation()

pytest.raises(ExceptionGroup, operation)  # error: [pytest-raises-exception-group]
pytest.raises(BaseExceptionGroup, operation, 1, value=2)  # error: [pytest-raises-exception-group]
pytest.raises(ValueError, operation, ExceptionGroup)
```

## Exception group backports

The `exceptiongroup` backport is checked on older Python versions too.

```toml
target-version = "py310"

[lint]
preview = true
select = ["ASYNC401"]
```

```py
import exceptiongroup
import exceptiongroup as eg
import pytest
from exceptiongroup import BaseExceptionGroup, ExceptionGroup
from exceptiongroup import BaseExceptionGroup as BEG, ExceptionGroup as EG

pytest.raises(exceptiongroup.ExceptionGroup)  # error: [pytest-raises-exception-group]
pytest.raises(exceptiongroup.BaseExceptionGroup)  # error: [pytest-raises-exception-group]
pytest.raises(eg.ExceptionGroup)  # error: [pytest-raises-exception-group]
pytest.raises(eg.BaseExceptionGroup)  # error: [pytest-raises-exception-group]
pytest.raises(ExceptionGroup)  # error: [pytest-raises-exception-group]
pytest.raises(BaseExceptionGroup)  # error: [pytest-raises-exception-group]
pytest.raises(EG)  # error: [pytest-raises-exception-group]
pytest.raises(BEG)  # error: [pytest-raises-exception-group]
pytest.raises(EG[Exception])  # error: [pytest-raises-exception-group]
pytest.raises(BEG[BaseException])  # error: [pytest-raises-exception-group]
pytest.RaisesExc(EG)  # error: [pytest-raises-exception-group]
pytest.RaisesGroup(BEG)  # error: [pytest-raises-exception-group]
pytest.mark.xfail(raises=EG)  # error: [pytest-raises-exception-group]
```

## Indirect and unsupported expressions

The rule does not infer exception types from assignments, subclasses, unpacking,
or other expressions. It only searches tuple literals for group types.

```py
import pytest

Group = ExceptionGroup
errors = (ValueError, ExceptionGroup)

class CustomGroup(ExceptionGroup):
    pass

pytest.raises(Group)
pytest.raises(errors)
pytest.raises(CustomGroup)
pytest.raises(*errors)
pytest.raises(**{"expected_exception": ExceptionGroup})
pytest.raises((ValueError, *errors))
pytest.raises(ExceptionGroup if condition else ValueError)
pytest.raises([ExceptionGroup])
pytest.raises({ExceptionGroup})
pytest.raises(ExceptionGroup("errors", [ValueError()]))
pytest.RaisesExc(Group)
pytest.RaisesExc(*errors)
pytest.RaisesGroup(CustomGroup)
pytest.RaisesGroup(*errors)
pytest.mark.xfail(raises=errors)
pytest.mark.xfail(**{"raises": ExceptionGroup})
```
