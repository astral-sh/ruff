# Unresolved Imports

## Unresolved import statement

```py
import bar # error: "Cannot resolve import `bar`"
```

## Unresolved import from statement

```py
from bar import baz # error: "Cannot resolve import `bar`"
```

## Unresolved import from resolved module

```py path=a.py
```

```py
from a import thing # error: "Module `a` has no member `thing`"
```

## Resolved import of symbol from unresolved import

```py path=a.py
import foo as foo # error: "Cannot resolve import `foo`"
```

```py
from a import foo # NOTE: Importing the unresolved import into a second first-party file should not trigger an additional "unresolved import" violation
```

## No implicit shadowing error

```py path=b.py
x: int
```

```py
from b import x

x = 'foo'  # error: "Object of type `Literal["foo"]"
```
