## What it does

Reports calls to `typing.NamedTuple` where the `fields` argument could supply valid field
definitions but ty cannot fully determine the fields from the argument.

## Why is this bad?

When ty cannot determine the complete set of field names, it may miss typos in constructor keyword
arguments and attribute names. An unrecognized attribute has type `Unknown` if it could refer to a
field with an indeterminate name.

ty can still check individual fields. For
`Record = NamedTuple("Record", [*fields, ("last", bytes)])`, ty checks that the argument for `last`
is a `bytes` whether passed positionally or by keyword, even if the length of `fields` is unknown.
On an instance `record`, both `record.last` and `record[-1]` have type `bytes`.

## Example

The type of a list variable doesn't tell ty how many fields the list contains.

```python
from typing import NamedTuple

fields = [("host", str), ("port", int)]
Url = NamedTuple("Url", fields)  # error: [non-literal-named-tuple-fields]
```

Writing the list directly in the call lets ty infer the fields. To keep the definitions in a
variable, use a fixed-length tuple with known field names and types.

```python
from typing import NamedTuple

fields = (("host", str), ("port", int))
Url = NamedTuple("Url", fields)
```

## Rule history

- v0.0.108: Split out from `invalid-named-tuple` as a standalone rule.
