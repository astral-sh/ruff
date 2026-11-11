## What it does

Reports calls to `typing.NamedTuple` where the field definitions have a compatible type but ty
cannot determine the fields statically.

## Why is this bad?

Python can create a named tuple from field definitions stored in a variable. Without literal field
definitions, ty cannot check arguments to the generated constructor or infer the types of the named
attributes.

## Example

```python
from typing import NamedTuple

fields = (("host", str), ("port", int))
Url = NamedTuple("Url", fields)  # error: [non-literal-named-tuple-fields]
```

Writing the fields directly in the call lets ty check them.

```python
from typing import NamedTuple

Url = NamedTuple("Url", [("host", str), ("port", int)])
```

## Rule history

- v0.0.102: Split out from `invalid-named-tuple`.
