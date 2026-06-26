# Recursive subscript and relation

```toml
[environment]
python-version = "3.12"
```

Recursive subscript operations should unfold one layer, perform the subscript, and fold the result
back under the same binder. Relation checks that compare recursive results should then unwrap only
one side at a time, so distinct recursive binders do not expand into each other.

## Recursive alias subscript

```py
type RecursiveList = list[RecursiveList]

def f(x: RecursiveList):
    reveal_type(x[0])  # revealed: list[RecursiveList]
    reveal_type(x[0][0])  # revealed: list[RecursiveList]
```

## Recursive relation for subscripted loop state

```py
class File:
    def readline(self) -> str:
        return ""

class TextFile:
    join_lines: bool
    skip_blanks: bool

    def __init__(self, file: File):
        self.file = file
        self.current_line = 0
        self.linebuf = []

    def readline(self):
        if self.linebuf:
            line = self.linebuf[-1]
            return line

        buildup_line = ""
        while True:
            line = self.file.readline()
            if line == "":
                line = None

            if self.join_lines and buildup_line:
                if line is None:
                    return buildup_line
                line = buildup_line + line
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

            reveal_type(line)  # revealed: str & ~Literal[""]
            reveal_type(self.current_line)  # revealed: int
            if line in ("", "\n") and self.skip_blanks:
                continue

            return line
```
