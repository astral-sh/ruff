# Recursive attribute tuple cycle stack overflow

This is minimized from a Dulwich ecosystem failure. Reading an instance attribute into local
variables and assigning a tuple containing those locals back to the same attribute used to build an
invalid nested recursive type and overflow the stack.

The suppressions below are for current precision diagnostics; the regression guarded here is
termination.

```py
class Reader:
    def __init__(self) -> None:
        self.contents = [b""]
        self.pos = (0, 0)  # ty: ignore[invalid-assignment]

    def read(self) -> None:
        chunk, cursor = self.pos

        while chunk < len(self.contents):  # ty: ignore[unsupported-operator]
            if cursor:
                cursor += 1
                self.pos = (chunk, cursor)
                break
            chunk += 1  # ty: ignore[unsupported-operator]
            cursor = 0
            self.pos = (chunk, cursor)  # ty: ignore[invalid-assignment]
```
