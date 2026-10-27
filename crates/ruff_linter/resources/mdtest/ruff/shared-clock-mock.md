# `shared-clock-mock`

```toml
lint.preview = true
lint.select = ["shared-clock-mock"]
```

## Shared sleep functions

Patching a shared module's sleep function can intercept unrelated callers, even
when the target is qualified with the consuming module.

```py
import time
from unittest.mock import patch

from example import worker

with patch("time.sleep"):  # snapshot: shared-clock-mock
    pass

with patch.object(time, "sleep"):  # error: [shared-clock-mock]
    pass

with patch("example.worker.time.sleep"):  # error: [shared-clock-mock]
    pass

with patch.object(worker.time, "sleep"):  # error: [shared-clock-mock]
    pass
```

```snapshot
error[shared-clock-mock]: Changing `time.sleep` may affect unrelated threads or tasks
 --> src/mdtest_snippet.py:6:12
  |
6 | with patch("time.sleep"):  # snapshot: shared-clock-mock
  |            ^^^^^^^^^^^^
help: Replace the binding in the consuming module with a private clock
```

## Clock readings

Unrelated callers can also consume a clock mock's finite sequence of readings.

```py
import time
from unittest.mock import patch

patch.object(time, "time")  # error: [shared-clock-mock]
patch.object(time, "time_ns")  # error: [shared-clock-mock]
patch.object(time, "monotonic")  # error: [shared-clock-mock]
patch.object(time, "monotonic_ns")  # error: [shared-clock-mock]
patch.object(time, "perf_counter")  # error: [shared-clock-mock]
patch.object(time, "perf_counter_ns")  # error: [shared-clock-mock]
patch("example.worker.time.perf_counter")  # error: [shared-clock-mock]

# Other time APIs are outside this rule's scope.
patch.object(time, "strftime")
patch("time.localtime")
```

## Import aliases and keyword arguments

```py
import asyncio as aio
import time as clock
from unittest import mock as testing
from unittest.mock import patch as replace

testing.patch.object(clock, "sleep")  # error: [shared-clock-mock]
testing.patch(target="asyncio.sleep")  # error: [shared-clock-mock]
replace.object(target=aio, attribute="sleep")  # error: [shared-clock-mock]
replace(target="time.monotonic", return_value=0)  # error: [shared-clock-mock]


@replace("example.worker.asyncio.sleep")  # error: [shared-clock-mock]
async def example(sleep):
    pass
```

## Pytest fixtures

The conventional fixture names identify pytest's monkeypatch API and pytest-mock's
patch APIs without requiring an import in the test file.

```py
import time

from example import worker


def example(monkeypatch, mocker, class_mocker, module_mocker, package_mocker, session_mocker):
    monkeypatch.setattr(time, "sleep", lambda _: None)  # error: [shared-clock-mock]
    monkeypatch.setattr(worker.time, "sleep", lambda _: None)  # error: [shared-clock-mock]
    monkeypatch.setattr(worker.time, "monotonic", lambda: 0)  # error: [shared-clock-mock]
    monkeypatch.setattr("example.worker.asyncio.sleep", fake_sleep)  # error: [shared-clock-mock]
    monkeypatch.setattr(target=time, name="perf_counter", value=lambda: 0)  # error: [shared-clock-mock]

    mocker.patch("time.sleep")  # error: [shared-clock-mock]
    class_mocker.patch("time.sleep")  # error: [shared-clock-mock]
    module_mocker.patch("time.sleep")  # error: [shared-clock-mock]
    package_mocker.patch("time.sleep")  # error: [shared-clock-mock]
    session_mocker.patch("time.sleep")  # error: [shared-clock-mock]

    mocker.patch.object(time, "sleep")  # error: [shared-clock-mock]
    class_mocker.patch.object(time, "sleep")  # error: [shared-clock-mock]
    module_mocker.patch.object(time, "sleep")  # error: [shared-clock-mock]
    package_mocker.patch.object(time, "sleep")  # error: [shared-clock-mock]
    session_mocker.patch.object(time, "sleep")  # error: [shared-clock-mock]
```

## Writes to clock attributes

Assigning or deleting a shared attribute changes it for every caller. Reading
the attribute, annotating it without a value, or replacing the consuming
module's binding does not.

```py
import asyncio as aio
import time as clock
from types import SimpleNamespace

from example import worker

clock.sleep = fake  # error: [shared-clock-mock]
worker.time.monotonic = fake  # error: [shared-clock-mock]
del aio.sleep  # error: [shared-clock-mock]
worker.time.time: object = fake  # error: [shared-clock-mock]

saved = worker.time.sleep
worker.time.sleep: object
worker.time = SimpleNamespace(sleep=fake)
```

Attribute targets also write to the shared module when bound by a loop, a
context manager, or a comprehension.

```py
import time

for time.sleep in replacements:  # error: [shared-clock-mock]
    pass
with replacement() as time.monotonic:  # error: [shared-clock-mock]
    pass
readings = [time.time() for time.time in clocks]  # error: [shared-clock-mock]
```

## Removing attributes with monkeypatch

```py
import time

def example(monkeypatch):
    monkeypatch.delattr(time, "sleep")  # error: [shared-clock-mock]
    monkeypatch.delattr("example.worker.time.monotonic")  # error: [shared-clock-mock]
    monkeypatch.delattr("example.worker.time")
```

## Constructed monkeypatch instances

```py
import time
from pytest import MonkeyPatch

mp = MonkeyPatch()
mp.setattr(time, "sleep", lambda _: None)  # error: [shared-clock-mock]
```

## Multiline patch targets

The diagnostic spans both the module and the attribute that will change.

```py
from example import worker

def test_retry(monkeypatch):
    monkeypatch.setattr(
        worker.time,  # snapshot: shared-clock-mock
        "sleep",
        lambda _: None,
    )
```

```snapshot
error[shared-clock-mock]: Changing `time.sleep` may affect unrelated threads or tasks
 --> src/mdtest_snippet.py:5:9
  |
5 | /         worker.time,  # snapshot: shared-clock-mock
6 | |         "sleep",
  | |_______________^
help: Replace the binding in the consuming module with a private clock
```

## Module-local replacements

Replacing the consuming module's binding leaves the shared module intact. A local
sleep binding and a directly constructed clock can also be patched independently.

```py
import asyncio
import time
from types import SimpleNamespace
from unittest.mock import AsyncMock, Mock, patch

from example import worker


def example(monkeypatch, mocker):
    monkeypatch.setattr(worker, "time", SimpleNamespace(sleep=Mock()))
    monkeypatch.setattr("example.worker.time", SimpleNamespace(sleep=Mock()))
    mocker.patch.object(worker, "time", SimpleNamespace(sleep=Mock()))
    mocker.patch("example.worker.time", SimpleNamespace(sleep=Mock()))
    monkeypatch.setattr(worker, "asyncio", SimpleNamespace(sleep=AsyncMock()))
    monkeypatch.setattr(worker, "sleep", Mock())
    mocker.patch("example.worker.sleep")


with patch.object(worker, "time", SimpleNamespace(sleep=Mock())):
    pass

with patch("example.worker.asyncio", SimpleNamespace(**(vars(asyncio) | {"sleep": AsyncMock()}))):
    pass

time = SimpleNamespace(sleep=Mock())
patch.object(time, "sleep")
```

## Unrelated methods and shadowed patch APIs

```py
import time
from unittest.mock import patch

from example import mocker, monkeypatch

mocker.patch("time.sleep")
monkeypatch.setattr(time, "sleep", fake)
other.patch("time.sleep")
other.setattr(time, "sleep", fake)


def example(patch):
    patch("time.sleep")
    patch.object(time, "sleep")


patch("asyncio.wait")
patch("example.sleep")
patch("example.clock.sleep")
patch.object(time, dynamic_attribute)
patch(dynamic_target)
patch(f"{module_name}.time.sleep")
```

## Attribute heuristic

Attribute targets ending in `time` or `asyncio` are treated as shared modules,
including on dynamically loaded modules. An already-isolated attribute is a known
false positive and can be suppressed by rule name.

```py
from types import SimpleNamespace
from unittest.mock import patch

module = load_module()
patch.object(module.time, "sleep")  # error: [shared-clock-mock]

module.time = SimpleNamespace(sleep=fake)
patch.object(module.time, "sleep")  # ruff: ignore[shared-clock-mock]
```

A clock stored directly on an instance or class is usually part of its own
state. A module stored on an instance can still expose the shared clock.

```py
from unittest.mock import patch

class Worker:
    def set_clock(self):
        self.time.sleep = fake
        patch.object(self.time, "monotonic")
        patch.object(self.module.time, "sleep")  # error: [shared-clock-mock]

    @classmethod
    def reset_clock(cls):
        del cls.asyncio.sleep
```
