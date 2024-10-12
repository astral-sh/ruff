# Follow imports

## Structures

### Class

We can follow import to class:

```py
from b import C as D; E = D
reveal_type(E) # revealed: Literal[C]
```

```py path=b.py
class C: pass
```

### Module member

```py
import b; D = b.C
reveal_type(D) # revealed: Literal[C]
```

```py path=b.py
class C: pass
```

## Relative

### Non-existent

Track that non-existent relative imports resolve to `Unknown`:

```py path=package/__init__.py
```

```py path=package/bar.py
from .foo import X # error: [unresolved-import]
reveal_type(X)  # revealed: Unknown
```

### Simple

We can follow relative imports:

```py path=package/__init__.py
```

```py path=package/foo.py
X = 42
```

```py path=package/bar.py
from .foo import X
reveal_type(X)  # revealed: Literal[42]
```

### Dotted

We can also follow dotted relative imports:

```py path=package/__init__.py
```

```py path=package/foo/bar/baz.py
X = 42
```

```py path=package/bar.py
from .foo.bar.baz import X
reveal_type(X)  # revealed: Literal[42]
```

### Bare to package

We can follow relative import bare to package:

```py path=package/__init__.py
X = 42
```

```py path=package/bar.py
from . import X
reveal_type(X)  # revealed: Literal[42]
```

### Non-existent + bare to package

```py path=package/bar.py
from . import X # error: [unresolved-import]
reveal_type(X)  # revealed: Unknown
```

### Dunder init

```py path=package/__init__.py
from .foo import X
reveal_type(X)  # revealed: Literal[42]
```

```py path=package/foo.py
X = 42
```

### Non-existent + dunder init

```py path=package/__init__.py
from .foo import X # error: [unresolved-import]
reveal_type(X)     # revealed: Unknown
```

### Long relative import

```py path=package/__init__.py
```

```py path=package/foo.py
X = 42
```

```py path=package/subpackage/subsubpackage/bar.py
from ...foo import X
reveal_type(X)  # revealed: Literal[42]
```

### Unbound symbol

We can track that imported unbound symbol is `Unknown`:

```py path=package/__init__.py
```

```py path=package/foo.py
x
```

```py path=package/bar.py
from .foo import x # error: [unresolved-import]
reveal_type(x)     # revealed: Unknown
```

### TODO: Bare to module

Submodule imports possibly not supported right now? Actually, `y` type should be `Literal[42]`.

```py path=package/__init__.py
```

```py path=package/foo.py
X = 42
```

```py path=package/bar.py
from . import foo  # error: [unresolved-import]
y = foo.X
reveal_type(y)     # revealed: Unknown
```

### TODO: Non-existent + bare to module

Submodule imports possibly not supported right now? Actually `foo` import should be resolved correctly.

```py path=package/__init__.py
```

```py path=package/bar.py
from . import foo  # error: [unresolved-import]
reveal_type(foo)   # revealed: Unknown
```

## Importing builtin module

```py
import builtins; x = builtins.copyright
reveal_type(x) # revealed: Literal[copyright]
```

## Import from stub declaration

We can infer types from `.pyi` stub files, where only a declaration exists without a definition:

```py
from b import x
y = x
reveal_type(y)  # revealed: int
```

```py path=b.pyi
x: int
```

## Import from non-stub with declaration and definition

When importing from a regular Python file, type declarations take priority over definitions:

```py
from b import x
y = x
reveal_type(y)  # revealed: int
```

```py path=b.py
x: int = 1
```

## Resolving errors

## Unresolved Imports

### Unresolved import statement

If a module cannot be found during an import, it will raise an error:

```py
import bar # error: "Cannot resolve import `bar`"
```

### Unresolved import from statement

Similar error when using `from ... import` syntax for a missing module:

```py
from bar import baz # error: "Cannot resolve import `bar`"
```

### Unresolved import from resolved module

If the module exists but the imported symbol does not, it raises a different error:

```py path=a.py
```

```py
from a import thing # error: "Module `a` has no member `thing`"
```

### Resolved import of symbol from unresolved import

If a symbol is imported from a module that itself cannot resolve imports, we still only raise an error in the unresolved module:

```py path=a.py
import foo as foo # error: "Cannot resolve import `foo`"
```

```py
from a import foo # NOTE: Importing the unresolved import into a second first-party file should not trigger an additional "unresolved import" violation
```

### No implicit shadowing error

When a variable is imported and then reassigned to an incompatible type, the type checker will raise an error about the mismatch:

```py path=b.py
x: int
```

```py
from b import x

x = 'foo'  # error: "Object of type `Literal["foo"]"
```

## Conditional

### Reimport

In cases where a module conditionally imports symbols from another module or provides its own definition, the type system should be able to infer the correct type from the relevant branch. However, disambiguation may still be required for complex cases.

TODO: We should disambiguate in such cases, showing `Literal[b.f, c.f]`.

```py path=c.py
def f(): ...
```

```py path=b.py
if flag:
    from c import f
else:
    def f(): ...
```

```py
from b import f # error: [invalid-assignment] "Object of type `Literal[f, f]` is not assignable to `Literal[f, f]`"
reveal_type(f)  # revealed: Literal[f, f]
```

### Reimport with stub declaration

When a conditional import involves both an import from another module and a local definition, the system correctly infers the type from the declared type of the imported symbol.

```py path=c.pyi
x: int
```

```py path=b.py
if flag:
    from c import x
else:
    x = 1
```

```py
from b import x 
reveal_type(x)  # revealed: int
```
