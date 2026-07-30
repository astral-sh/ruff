# ty #4125: inherited dataclass field-order parity

Issue: <https://github.com/astral-sh/ty/issues/4125>

## Goal

Report a dataclass field-order error when a required positional constructor parameter follows an
inherited positional parameter with a default. Match established behavior in mypy and Pyright
without attempting to model every runtime-valid dataclass edge case more accurately than either
checker.

The relevant rule is about the generated `__init__`, not simply the order of annotations inside one
class body.

## Checker versions and reproduction

The comparisons below were checked against:

- mypy 2.3.0
- Pyright 1.1.411
- Python 3.12 typing semantics

Run the same comparison with:

```sh
uvx mypy --python-version 3.12 --show-error-codes example.py
uvx pyright --pythonversion 3.12 example.py
cargo run --bin ty -- check --output-format=concise example.py
```

Diagnostic wording and rule names differ across checkers. Parity here means agreement about the
substantive behavior, not identical text or identical diagnostics for unrelated override errors.

## Required behavior

### Required fields after inherited defaults

The original issue is a defaulted field in a base dataclass followed by a required field in a child
dataclass:

```py
from dataclasses import dataclass


@dataclass
class Base:
    first: int = 1


@dataclass
class Child(Base):
    second: int
```

Python raises `TypeError` while generating `Child.__init__`. Both mypy and Pyright report the
ordering problem, so ty must report `dataclass-field-order` on `second`.

The same rule applies when the inherited default comes from `field(default_factory=...)`:

```py
from dataclasses import dataclass, field


@dataclass
class Base:
    first: list[int] = field(default_factory=list)


@dataclass
class Child(Base):
    second: int
```

Both comparison checkers reject this example.

### Dataclass-transform inheritance

Dataclass-like classes created with `@dataclass_transform` follow the same constructor ordering
rule:

```py
from typing import dataclass_transform


@dataclass_transform()
def transform[T](cls: type[T]) -> type[T]:
    return cls


@transform
class Base:
    first: int = 1


@transform
class Child(Base):
    second: int
```

Both mypy and Pyright report an ordering error on `second`.

### Fields that do not participate in positional ordering

Keyword-only fields and fields excluded from `__init__` must not make a later positional field
invalid:

```py
from dataclasses import dataclass, field


@dataclass
class KeywordOnlyBase:
    optional: int = field(default=1, kw_only=True)


@dataclass
class KeywordOnlyChild(KeywordOnlyBase):
    required: int


@dataclass
class NonInitBase:
    optional: int = field(default=1, init=False)


@dataclass
class NonInitChild(NonInitBase):
    required: int
```

Both mypy and Pyright accept these examples.

### Newly introduced ordering violations

An ancestor can already have a suppressed ordering violation. A subclass can subsequently create a
different ordering violation involving the same required field name:

```py
from dataclasses import dataclass, field


@dataclass
class Base:
    first: int = 1
    second: int  # type: ignore
    third: int  # type: ignore


@dataclass
class Child(Base):
    first: int = field()
    second: int = 1
    third: int = field()
```

The base's violations are caused by `first` preceding `second` and `third`. The child's new
violation is caused by the newly defaulted `second` preceding the newly required `third`.

Both mypy and Pyright report the subclass violation. Pyright also reports a separate missing-default
override error. ty must not discard the subclass ordering error simply because an ancestor had a
violation involving the name `third`.

Deduplication should therefore compare the defaulted field, the required field, and their
declaration provenance. It must not compare the required field name alone.

### Existing behavior that should remain intact

- A violation already present in an ancestor should not be repeated on unchanged descendants. This
    matches Pyright, although mypy repeats the diagnostic.
- Conditional declarations must respect suppression independently on each reachable declaration.
- A child that first enables constructor generation can expose an inherited ordering violation.
- Multiple inheritance can combine otherwise valid bases into an invalid constructor. mypy reports
    this; Pyright currently does not.
- An inherited field replaced by `ClassVar` should not cause an additional field-order diagnostic.
    Pyright reports only the incompatible override; mypy also reports ordering and call errors.

## Comparison matrix

| Scenario                                               | Runtime                                       | mypy 2.3.0                          | Pyright 1.1.411                | Desired ty behavior                          |
| ------------------------------------------------------ | --------------------------------------------- | ----------------------------------- | ------------------------------ | -------------------------------------------- |
| Required child field after inherited default           | Invalid                                       | Ordering error                      | Ordering error                 | Ordering error                               |
| Required child field after inherited default factory   | Invalid                                       | Ordering error                      | Ordering error                 | Ordering error                               |
| Inherited `@dataclass_transform` default               | Invalid constructor shape                     | Ordering error                      | Ordering error                 | Ordering error                               |
| Keyword-only or `init=False` base field                | Valid                                         | Accepted                            | Accepted                       | Accepted                                     |
| New subclass violation with an already-seen field name | Invalid                                       | Ordering error                      | Ordering error                 | Ordering error                               |
| Unchanged descendants of an ignored invalid ancestor   | Invalid ancestor                              | Repeated errors                     | Accepted                       | No repeated error                            |
| Violation introduced by multiple inheritance           | Invalid                                       | Ordering error                      | Accepted                       | Ordering error                               |
| `ClassVar` overriding an inherited instance field      | Invalid override; constructor itself is valid | Override, ordering, and call errors | Override error only            | Override error only                          |
| Annotation-only override of a defaulted field          | Valid                                         | Ordering error                      | Missing-default override error | Existing error is acceptable                 |
| Conditional `ClassVar`/`InitVar` declarations          | Depends on the branch                         | Redefinition error                  | Override and call errors       | Existing conservative behavior is acceptable |
| `ClassVar` pseudo-field restored as `InitVar`          | Valid                                         | Ordering error                      | Ordering and override errors   | Existing error is acceptable                 |

## Explicitly out of scope

### Preserving inherited defaults on annotation-only overrides

Python accepts this class because `second` retains its inherited runtime default:

```py
from dataclasses import dataclass


@dataclass
class Base:
    first: int = 1
    second: int = 2


@dataclass
class Child(Base):
    second: int
```

mypy reports an ordering error. Pyright reports that the override is missing a default. Matching
Python more precisely here would exceed parity with both checkers.

### Path-sensitive `ClassVar` versus `InitVar` constructor signatures

```py
from dataclasses import InitVar, dataclass
from typing import ClassVar


def condition() -> bool:
    return False


@dataclass
class Base:
    value: InitVar[int]
    other: int


@dataclass
class Child(Base):
    if condition():
        value: ClassVar[int] = 1
    else:
        value: InitVar[int]


Child(1, 2)
```

mypy reports a redefinition. Pyright rejects the override and the two-argument call. Building
path-sensitive constructor alternatives or a union of generated signatures is not required.

### Preserving `ClassVar` pseudo-field positions

```py
from dataclasses import InitVar, dataclass
from typing import ClassVar


@dataclass
class Base:
    first: ClassVar[int]
    later: int = 2


@dataclass
class Child(Base):
    first: InitVar[int]
```

Python accepts this example, but both mypy and Pyright report a field-order error. Adding a new
ordered pseudo-field representation solely to accept it is outside the issue's parity goal.

### General non-goals

- Exact diagnostic wording or error-code equivalence across checkers.
- Reproducing every mypy behavior when Pyright disagrees, or vice versa.
- Correcting every mismatch between static dataclass models and CPython runtime behavior.
- Introducing a new flow-sensitive dataclass layout, constructor-signature unions, or pseudo-field
    position tracking solely for cases both established checkers already reject.
- Expanding this issue into a general redesign of `ClassVar`, `InitVar`, inherited defaults, or
    dataclass-transform semantics.
