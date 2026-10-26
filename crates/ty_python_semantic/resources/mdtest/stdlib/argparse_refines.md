# Inferring argparse namespace attributes

For a positional argument, argparse uses the argument name as the namespace attribute. This stub
models required positional arguments with a `type` converter. Each `add_argument` call adds one
`Argument[Name, T]` to the parser's schema. `Namespace.__getattr__` uses that schema to look up the
value type for a given attribute name.

```toml
[environment]
python-version = "3.14"
```

`argparse.pyi`:

```pyi
from typing import Callable, LiteralString, Protocol
from ty_extensions.experimental import Refines

class Argument[Name: LiteralString, T](Protocol):
    name: Name
    value: T

class Namespace[Row]:
    def __getattr__[Name: LiteralString, T](self: Namespace[Argument[Name, T]], name: Name) -> T: ...

class Action: ...

class ArgumentParser[Row = object]:
    def add_argument[Name: LiteralString, T](
        self, name: Name, *, type: Callable[[str], T]
    ) -> Refines[ArgumentParser[Row & Argument[Name, T]], Action]: ...
    def parse_args(self) -> Namespace[Row]: ...
```

```py
from argparse import ArgumentParser

parser = ArgumentParser()
parser.add_argument("host", type=str)
parser.add_argument("port", type=int)
args = parser.parse_args()

reveal_type(args.host)  # revealed: str
reveal_type(args.port)  # revealed: int

args.missing  # error: [invalid-attribute-access]
```
