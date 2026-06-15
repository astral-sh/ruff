# Recursive member lookup convergence

```toml
[environment]
python-version = "3.12"
```

Cycle recovery for implicit instance attributes should keep union element order stable when
merging bodyful recursive markers. Otherwise member lookup participants can oscillate between
equivalent unions.

## Attribute cycle through member lookup and unpacking

```py
import collections


class messages:
    class UndefinedName:
        message_args = ()


class Scope(dict[str, object]):
    indirect_assignments = {}


class Assignment:
    def __init__(self, name, source):
        self.name = name
        self.source = source
        self.used = None


class Checker:
    offset = None

    def __init__(self, filename="(none)"):
        self._deferred = collections.deque()
        self.messages = []
        self.filename = filename
        self.scopeStack = [Scope()]
        self._run_deferred()

    def _run_deferred(self):
        orig = (self.scopeStack, self.offset)

        while self._deferred:
            handler, scope, offset = self._deferred.popleft()
            self.scopeStack, self.offset = scope, offset
            handler()

        self.scopeStack, self.offset = orig

    def handleDoctests(self, node):
        docstring, node_lineno = self.getDocstring(node)
        examples = docstring and self._getDoctestExamples(docstring)
        if not examples:
            return

        node_offset = self.offset or (0, 0)
        for example in examples:
            self.offset = (
                node_offset[0] + node_lineno + example.lineno,
                node_offset[1] + example.indent + 4,
            )
            self.offset = node_offset

    def GLOBAL(self, node):
        global_scope_index = 1 if self._in_doctest() else 0
        global_scope = self.scopeStack[global_scope_index]

        if self.scope is not global_scope:
            for node_name in node.names:
                node_value = Assignment(node_name, node)
                self.messages = [
                    m
                    for m in self.messages
                    if not isinstance(m, messages.UndefinedName)
                ]
                global_scope.setdefault(node_name, node_value)
                node_value.used = (global_scope, node)
                for scope in self.scopeStack[global_scope_index + 1 :]:
                    scope[node_name] = node_value
                self.scope.indirect_assignments[node_name] = node

    @property
    def scope(self):
        return self.scopeStack[-1]

    def _in_doctest(self):
        return False

    def getDocstring(self, node):
        return "", 0

    def _getDoctestExamples(self, docstring):
        return ()


def run(checker: Checker):
    return checker.messages
```
