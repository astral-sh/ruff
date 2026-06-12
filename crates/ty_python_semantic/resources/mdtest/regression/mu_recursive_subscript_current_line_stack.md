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
        reveal_type(line)  # revealed: (Unknown & ~None) | int | list[int]

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
