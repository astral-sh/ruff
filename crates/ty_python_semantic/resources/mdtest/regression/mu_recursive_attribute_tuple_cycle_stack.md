# Recursive attribute tuple cycle stack overflow

This is minimized from a Dulwich ecosystem failure. Reading an instance attribute into local
variables and assigning a tuple containing those locals back to the same attribute used to build an
invalid nested recursive type and overflow the stack.

```py
class Reader:
    def __init__(self) -> None:
        self.contents = [b""]
        self.pos = (0, 0)

    def read(self) -> None:
        chunk, cursor = self.pos

        while chunk < len(self.contents):
            if cursor:
                cursor += 1
                self.pos = (chunk, cursor)
                break
            chunk += 1
            cursor = 0
            self.pos = (chunk, cursor)

        reveal_type(self.pos)  # revealed: Divergent
```
