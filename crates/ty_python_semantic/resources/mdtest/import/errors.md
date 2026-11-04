# Unresolved Imports

## Unresolved import statement

```py
import bar  # error: "Cannot resolve imported module `bar`"

reveal_type(bar)  # revealed: Unknown
```

## Unresolved import from statement

```py
from bar import baz  # error: "Cannot resolve imported module `bar`"

reveal_type(baz)  # revealed: Unknown
```

## Unresolved import from resolved module

`a.py`:

```py
```

```py
from a import thing  # error: "Module `a` has no member `thing`"

reveal_type(thing)  # revealed: Unknown
```

## Suppressing `import-not-found` with the previous rule name

`unresolved-import` is an alias for `import-not-found`. Existing ignore comments that use the old
name still suppress errors for both a missing module and a missing member.

```py
import missing_module  # ty: ignore[unresolved-import]
from math import missing_member  # type: ignore[ty:unresolved-import]
```

## Reporting an unused `unresolved-import` suppression

In the following example, only `ty: ignore[unresolved-reference]` is required to suppress the
diagnostic for the unresolved name on `print(missing)`. The `unresolved-import` suppression is
unused. But `unresolved-import` is actually a legacy alias for `import-not-found` -- a naive
implementation of `unused-ignore-comment` could result in us, confusingly, complaining that an
"`import-not-found`" suppression comment was unused, even though none exists!

This test enforces that we use the name of the rule which was actually used in the source code when
we complain about unused ignore comments, even if the name used in the source code is merely a
legacy alias to the rule's canonical name.

```py
# error: [unused-ignore-comment] "Unused `ty: ignore` directive: 'unresolved-import'"
print(missing)  # ty: ignore[unresolved-reference, unresolved-import]

# error: [unused-type-ignore-comment] "'ty:division-by-zero', 'ty:unresolved-import'"
print(missing)  # type: ignore[ty:unresolved-reference, ty:division-by-zero, ty:unresolved-import]
```

## Resolved import of symbol from unresolved import

`a.py`:

```py
import foo as foo  # error: "Cannot resolve imported module `foo`"

reveal_type(foo)  # revealed: Unknown
```

Importing the unresolved import into a second file should not trigger an additional "unresolved
import" violation:

```py
from a import foo

reveal_type(foo)  # revealed: Unknown
```

## Imported annotations do not constrain local assignments

`b.py`:

```py
x: int
```

```py
from b import x

x = "foo"
reveal_type(x)  # revealed: Literal["foo"]
```

## Import cycle

`a.py`:

```py
from ty_extensions._internal import reveal_mro

class A: ...

reveal_mro(A)  # revealed: (<class 'A'>, <class 'object'>)
import b

class C(b.B): ...

reveal_mro(C)  # revealed: (<class 'C'>, <class 'B'>, <class 'A'>, <class 'object'>)
```

`b.py`:

```py
from ty_extensions._internal import reveal_mro
from a import A

class B(A): ...

reveal_mro(B)  # revealed: (<class 'B'>, <class 'A'>, <class 'object'>)
```
