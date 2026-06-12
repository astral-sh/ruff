# Recursive attribute tuple cycle stack overflow

This is minimized from a Dulwich ecosystem failure. Reading an instance attribute into local
variables and assigning a tuple containing those locals back to the same attribute used to build an
invalid nested recursive type and overflow the stack.

```py
class Reader:
    def __init__(self) -> None:
        self.contents = [b""]
        # error: [invalid-assignment] "Object of type `tuple[Literal[0], Literal[0]]` is not assignable to attribute `pos` of type `tuple[Divergent, int] | tuple[Divergent, Divergent] | tuple[Divergent, int | Unknown]`"
        self.pos = (0, 0)

    def read(self) -> None:
        chunk, cursor = self.pos

        # error: [unsupported-operator] "Operator `<` is not supported between objects of type `tuple[Divergent, int] | tuple[Divergent, Divergent] | tuple[Divergent, int | Unknown] | int | Unknown` and `int`"
        while chunk < len(self.contents):
            if cursor:
                # error: [unsupported-operator] "Operator `+=` is not supported between objects of type `tuple[Divergent, Divergent]` and `Literal[1]`"
                # error: [unsupported-operator] "Operator `+=` is not supported between objects of type `tuple[Divergent, int | Unknown]` and `Literal[1]`"
                cursor += 1
                # error: [invalid-assignment] "Object of type `tuple[tuple[Divergent, int] | tuple[Divergent, Divergent] | tuple[Divergent, int | Unknown] | int | Unknown, int | Unknown]` is not assignable to attribute `pos` of type `tuple[Divergent, int] | tuple[Divergent, Divergent] | tuple[Divergent, int | Unknown]`"
                self.pos = (chunk, cursor)
                break
            # error: [unsupported-operator] "Operator `+=` is not supported between objects of type `tuple[Divergent, int] | tuple[Divergent, Divergent] | tuple[Divergent, int | Unknown]` and `Literal[1]`"
            chunk += 1
            cursor = 0
            # error: [invalid-assignment] "Object of type `tuple[Unknown | int, Literal[0]]` is not assignable to attribute `pos` of type `tuple[Divergent, int] | tuple[Divergent, Divergent] | tuple[Divergent, int | Unknown]`"
            self.pos = (chunk, cursor)

        reveal_type(self.pos)  # revealed: tuple[Divergent, int] | tuple[Divergent, Divergent] | tuple[Divergent, int | Unknown]
```
