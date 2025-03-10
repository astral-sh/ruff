# Custom typeshed

The `environment.typeshed` configuration option can be used to specify a custom typeshed directory
for Markdown-based tests. Custom typeshed stubs can then be placed in the specified directory using
fenced code blocks with language `pyi`, and will be used instead of the vendored copy of typeshed.

A fenced code block with language `text` can be used to provide a `stdlib/VERSIONS` file in the
custom typeshed root. If no such file is created explicitly, it will be automatically created with
entries enabling all specified `<typeshed-root>/stdlib` files for all supported Python versions.

## Basic example (auto-generated `VERSIONS` file)

First, we specify `/typeshed` as the custom typeshed directory:

```toml
[environment]
typeshed = "/typeshed"
```

We can then place custom stub files in `/typeshed/stdlib`, for example:

`/typeshed/stdlib/builtins.pyi`:

```pyi
class BuiltinClass: ...

builtin_symbol: BuiltinClass
```

`/typeshed/stdlib/sys/__init__.pyi`:

```pyi
version = "my custom Python"
```

And finally write a normal Python code block that makes use of the custom stubs:

```py
b: BuiltinClass = builtin_symbol

class OtherClass: ...

o: OtherClass = builtin_symbol  # error: [invalid-assignment]

# Make sure that 'sys' has a proper entry in the auto-generated 'VERSIONS' file
import sys
```

## Custom `VERSIONS` file

If we want to specify a custom `VERSIONS` file, we can do so by creating a fenced code block with
language `text`. In the following test, we set the Python version to `3.10` and then make sure that
we can *not* import `new_module` with a version requirement of `3.11-`:

```toml
[environment]
python-version = "3.10"
typeshed = "/typeshed"
```

`/typeshed/stdlib/old_module.pyi`:

```pyi
class OldClass: ...
```

`/typeshed/stdlib/new_module.pyi`:

```pyi
class NewClass: ...
```

`/typeshed/stdlib/VERSIONS`:

```text
old_module: 3.0-
new_module: 3.11-
```

```py
from old_module import OldClass

# error: [unresolved-import] "Cannot resolve import `new_module`"
from new_module import NewClass
```

## Using `reveal_type` with a custom typeshed

When providing a custom typeshed directory, basic things like `reveal_type` will stop working
because we rely on being able to import it from `typing_extensions`. The actual definition of
`reveal_type` in typeshed is slightly involved (depends on generics, `TypeVar`, etc.), but a very
simple untyped definition is enough to make `reveal_type` work in tests:

```toml
[environment]
typeshed = "/typeshed"
```

`/typeshed/stdlib/typing_extensions.pyi`:

```pyi
def reveal_type(obj, /): ...
```

```py
reveal_type(())  # revealed: tuple[()]
```
