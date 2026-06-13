# Recursive attribute subscript stack overflow

This is minimized from Setuptools and SymPy ecosystem failures. A loop-carried attribute can
alternate between an integer line number and a pair containing the previous line state. Reading an
indexed element from that recursive attribute must terminate.

```py
class TextFile:
    def __init__(self, join_lines: bool) -> None:
        self.join_lines = join_lines
        self.current_line = 0

    def gen_error(self, line=None):
        if line is None:
            line = self.current_line
        reveal_type(line)  # revealed: (Unknown & ~None) | list[int] | int

    def readline(self, line: str | None, buildup_line: str):
        while True:
            if self.join_lines and buildup_line:
                if isinstance(self.current_line, list):
                    self.current_line[1] = self.current_line[1] + 1
                else:
                    self.current_line = [self.current_line, self.current_line + 1]
            else:
                if line is None:
                    return None
                if isinstance(self.current_line, list):
                    self.current_line = self.current_line[1] + 1
                else:
                    self.current_line = self.current_line + 1
```

The line state can also include `None` on some paths. If the attribute enters a list state inside
the loop and then returns to the element type through a subscript read, checking the assignment
should terminate and report the invalid attribute assignment.

```py
class SetuptoolsCurrentLineState:
    def __init__(self) -> None:
        self.current_line = 0
        self.current_line = None

    def update(self) -> None:
        while True:
            if not isinstance(self.current_line, list):
                self.current_line = [self.current_line, self.current_line]  # error: [invalid-assignment]

            if isinstance(self.current_line, list):
                self.current_line = self.current_line[1]
```

Assigning a member to itself in the same loop does not add new type information and should not
force the member to be inferred from its own binding.

```py
class MemberSelfAssignmentNoOp:
    def __init__(self) -> None:
        self.current_line = 0
        self.current_line = None

    def update(self) -> None:
        while True:
            if not isinstance(self.current_line, list):
                self.current_line = [self.current_line, self.current_line]

            if isinstance(self.current_line, list):
                self.current_line = self.current_line
```

This is minimized from a Tanjun ecosystem failure. The recursive tree alias can flow through a
loop-carried local, be read with a subscript, and then be used for a member lookup. Cycle recovery
should not preserve markers on dynamic fallback values produced by the subscript operation.

```py
class MessageCommand:
    pass

class _IndexKeys:
    COMMANDS = object()

_TreeT = dict[str | object, "_TreeT | list[tuple[list[str], MessageCommand]]"]

class MessageCommandIndex:
    search_tree: _TreeT | None = None

    def find(self, split: list[str]) -> None:
        if self.search_tree is None:
            return

        node: _TreeT | list[tuple[list[str], MessageCommand]]
        node = self.search_tree
        for chars in split:
            try:
                node = node[chars.casefold()]
            except KeyError:
                break
            else:
                assert isinstance(node, dict)
                if entries := node.get(_IndexKeys.COMMANDS):
                    assert isinstance(entries, list)
```
