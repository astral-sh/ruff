# Regression test for #3720

Regression test for [this issue](https://github.com/astral-sh/ty/issues/3720).

Computing the code generator (dataclass-like / namedtuple-like behavior) of a dynamically created
class (`type(...)`) whose bases refer back to the class could form a Salsa query cycle through
`code_generator_of_dynamic_class`, panicking with `dependency graph cycle`. The query now recovers
from cycles the same way its static-class sibling does (`cycle_initial=|_, _, _| None`), so checking
this fuzzer-discovered snippet must not panic.

```toml
[environment]
python-version = "3.12"
```

```py
from abc import ABC
from typing import NamedTuple

# error: [invalid-argument-type] "Invalid argument to parameter 2 (`bases`) of `type()`"
# error: [invalid-named-tuple] "is not a valid identifier"
T = type("T", NamedTuple("T", [("", "T")]), {})
T()
T = ABC
```
