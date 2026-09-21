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
error[ASYNC401]: Use `pytest.RaisesGroup` instead of `pytest.raises` for exception groups
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
```
