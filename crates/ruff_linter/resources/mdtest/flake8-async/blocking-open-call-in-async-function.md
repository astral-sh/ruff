# `blocking-open-call-in-async-function` (`ASYNC230`)

```toml
lint.select = ["ASYNC230"]
```

## Builtin imports

Opening a file blocks an async function whether `open` is referenced directly, through `builtins`,
or through an imported alias. `io.open` is also a blocking call.

```py
import builtins
import io
from builtins import open as builtin_open

async def read_file():
    open("data.txt")  # error: [blocking-open-call-in-async-function]
    builtins.open("data.txt")  # error: [blocking-open-call-in-async-function]
    builtin_open("data.txt")  # error: [blocking-open-call-in-async-function]
    io.open("data.txt")  # error: [blocking-open-call-in-async-function]
```

## Synchronous functions

The rule only checks calls in async contexts.

```py
import builtins

def read_file():
    builtins.open("data.txt")
```

## Shadowed names

A parameter named `open` does not refer to the builtin.

```py
async def custom_open(open):
    open("data.txt")
```
