# Declarations in stubs

Unlike regular Python modules, stub files often declare module-global variables without initializing
them. If these symbols are then used in the same stub, applying regular logic would lead to an
undefined variable access error.

However, from the perspective of the type checker, we should treat something like `symbol: type` the
same as `symbol: type = ...`. In other words, assume these are bindings too.

```pyi
from typing import Literal

CONSTANT: Literal[42]

# No error here, even though the variable is not initialized.
uses_constant: int = CONSTANT
```
