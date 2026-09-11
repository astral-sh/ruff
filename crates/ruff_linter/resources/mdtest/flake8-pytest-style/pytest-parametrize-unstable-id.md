# `pytest-parametrize-unstable-id` (`PT901`)

```toml
[lint]
preview = true
select = ["PT901"]
```

## Dynamically generated values

Values produced by calls and names can generate unstable test IDs when pytest
collects the same tests in different processes.

```py
from uuid import uuid4

import pytest

value = str(uuid4())

@pytest.mark.parametrize("value", [str(uuid4()), "invalid"])  # error: [pytest-parametrize-unstable-id]
def test_generated_value(value): ...

@pytest.mark.parametrize("value", [value])  # error: [pytest-parametrize-unstable-id]
def test_named_value(value): ...
```

## Literal values

Scalar literals, signed numbers, and collection literals have stable pytest IDs.

```py
from uuid import uuid4

import pytest

@pytest.mark.parametrize(
    "value",
    ["value", b"value", 1, -1, 1.5, True, False, None, ..., [1], (1,), {1}, {"key": 1}],
)
def test_literal_values(value): ...
```

pytest does not derive an ID from a collection-valued parameter's contents. It
instead uses the parameter name and index, so the ID remains stable even when
the collection contains a dynamically generated value. Comprehensions behave the
same way, and generator expressions use a stable name-based ID. See [pytest's
test ID documentation][pytest-test-ids].

```py
@pytest.mark.parametrize(
    "value",
    [[str(uuid4())], (str(uuid4()),), {str(uuid4())}, {"value": str(uuid4())}],
)
def test_collection_values(value): ...

@pytest.mark.parametrize("value", [[str(uuid4()) for _ in range(1)]])
def test_list_comprehension(value): ...

@pytest.mark.parametrize("value", [{str(uuid4()) for _ in range(1)}])
def test_set_comprehension(value): ...

@pytest.mark.parametrize("value", [{"value": str(uuid4()) for _ in range(1)}])
def test_dict_comprehension(value): ...

@pytest.mark.parametrize("value", [(str(uuid4()) for _ in range(1))])
def test_generator_expression(value): ...
```

## IDs on individual parameter sets

An ID on `pytest.param` stabilizes the collected test regardless of the value.
Without an ID, dynamically generated values are still reported.

```py
from uuid import uuid4

import pytest
from pytest import param as parameter

@pytest.mark.parametrize("value", [pytest.param(str(uuid4()), id="generated")])
def test_explicit_id(value): ...

@pytest.mark.parametrize("value", [parameter(str(uuid4()), id="generated")])
def test_aliased_param(value): ...

@pytest.mark.parametrize("value", [pytest.param("literal", marks=pytest.mark.xfail)])
def test_literal_without_id(value): ...

@pytest.mark.parametrize("value", [pytest.param(str(uuid4()))])  # error: [pytest-parametrize-unstable-id]
def test_missing_id(value): ...

@pytest.mark.parametrize("value", [pytest.param(str(uuid4()), id=None)])  # error: [pytest-parametrize-unstable-id]
def test_none_id(value): ...
```

## IDs on the parametrization decorator

An explicit list, tuple, callable, or positional `ids` argument provides stable
test IDs. A `None` entry still requests pytest's generated ID for that case.

```py
from uuid import uuid4

import pytest

def make_id(value):
    return "generated"

@pytest.mark.parametrize("value", [str(uuid4())], ids=["generated"])
def test_list_ids(value): ...

@pytest.mark.parametrize("value", [str(uuid4())], ids=("generated",))
def test_tuple_ids(value): ...

@pytest.mark.parametrize("value", [str(uuid4())], ids=make_id)
def test_callable_ids(value): ...

@pytest.mark.parametrize("value", [str(uuid4())], False, ["generated"])
def test_positional_ids(value): ...

@pytest.mark.parametrize("value", [str(uuid4())], ids=None)  # error: [pytest-parametrize-unstable-id]
def test_none_ids(value): ...

@pytest.mark.parametrize("value", [str(uuid4()), "literal"], ids=[None, "literal"])  # error: [pytest-parametrize-unstable-id]
def test_none_id_entry(value): ...
```

## Multiple parameters

A parameter row needs one explicit ID when any of its values is non-literal.
Only one diagnostic is emitted for each row.

```py
from uuid import uuid4

import pytest

@pytest.mark.parametrize(("first", "second"), [("literal", str(uuid4()))])  # error: [pytest-parametrize-unstable-id]
def test_tuple_row(first, second): ...

@pytest.mark.parametrize("first, second", [[str(uuid4()), str(uuid4())]])  # error: [pytest-parametrize-unstable-id]
def test_list_row(first, second): ...

@pytest.mark.parametrize(("first", "second"), [("first", "second")])
def test_literal_row(first, second): ...

@pytest.mark.parametrize(
    ("first", "second"),
    [pytest.param(str(uuid4()), str(uuid4()), id="generated")],
)
def test_explicit_row_id(first, second): ...
```

## Single-parameter rows

When parameter names are passed as a sequence, pytest unpacks each parameter row
even if there is only one parameter.

```py
from uuid import uuid4

import pytest

@pytest.mark.parametrize(("value",), [(str(uuid4()),)])  # error: [pytest-parametrize-unstable-id]
def test_tuple_name(value): ...

@pytest.mark.parametrize(["value"], [[str(uuid4())]])  # error: [pytest-parametrize-unstable-id]
def test_list_name(value): ...
```

## Keyword arguments and unknown inputs

Both argument styles are supported. Unknown parameter sets, unpacked parameter
sets, and unpacked keyword arguments are ignored because their IDs cannot be
determined statically.

```py
from uuid import uuid4

import pytest

cases = [pytest.param(str(uuid4()), id="generated")]
options = {"ids": ["generated"]}

@pytest.mark.parametrize(argnames="value", argvalues=[str(uuid4())])  # error: [pytest-parametrize-unstable-id]
def test_keyword_arguments(value): ...

@pytest.mark.parametrize("value", cases)
def test_unknown_cases(value): ...

@pytest.mark.parametrize("value", [*cases])
def test_unpacked_cases(value): ...

@pytest.mark.parametrize("value", [*cases, str(uuid4())])
def test_unpacked_cases_with_generated_value(value): ...

@pytest.mark.parametrize("value", [str(uuid4())], ids=[*options["ids"]])
def test_unpacked_ids(value): ...

@pytest.mark.parametrize("value", [str(uuid4())], **options)
def test_unpacked_options(value): ...
```

[pytest-test-ids]: https://docs.pytest.org/en/stable/example/parametrize.html#different-options-for-test-ids
