# `pytest-unittest-assertion` (`PT009`)

```toml
[lint]
select = ["PT009"]
```

## Regex assertions add an import

All four regex assertion methods need a usable `re.search` binding in the replacement.

```py
import unittest

class Test(unittest.TestCase):
    def check(self):
        self.assertRegex("abc", "a")  # snapshot: pytest-unittest-assertion
        self.assertNotRegex("abc", "z")  # error: [pytest-unittest-assertion]
        self.assertRegexpMatches("abc", "a")  # error: [pytest-unittest-assertion]
        self.assertNotRegexpMatches("abc", "z")  # error: [pytest-unittest-assertion]
```

```snapshot
error[PT009]: Use a regular `assert` instead of unittest-style `assertRegex`
 --> src/mdtest_snippet.py:5:9
  |
5 |         self.assertRegex("abc", "a")  # snapshot: pytest-unittest-assertion
  |         ^^^^^^^^^^^^^^^^
help: Replace `assertRegex(...)` with `assert ...`
  |
1 | import unittest
2 + import re
3 |
4 | class Test(unittest.TestCase):
5 |     def check(self):
  -         self.assertRegex("abc", "a")  # snapshot: pytest-unittest-assertion
6 +         assert re.search("a", "abc")  # snapshot: pytest-unittest-assertion
7 |         self.assertNotRegex("abc", "z")  # error: [pytest-unittest-assertion]
  |
note: This is an unsafe fix and may change runtime behavior
```

## Existing module import

```py
import re

class Test:
    def check(self):
        self.assertRegex("abc", "a")  # snapshot: pytest-unittest-assertion
```

```snapshot
error[PT009]: Use a regular `assert` instead of unittest-style `assertRegex`
 --> src/mdtest_snippet.py:5:9
  |
5 |         self.assertRegex("abc", "a")  # snapshot: pytest-unittest-assertion
  |         ^^^^^^^^^^^^^^^^
help: Replace `assertRegex(...)` with `assert ...`
  |
4 |     def check(self):
  -         self.assertRegex("abc", "a")  # snapshot: pytest-unittest-assertion
5 +         assert re.search("a", "abc")  # snapshot: pytest-unittest-assertion
  |
note: This is an unsafe fix and may change runtime behavior
```

## Existing module alias

```py
import re as regex

class Test:
    def check(self):
        self.assertNotRegex("abc", "z")  # snapshot: pytest-unittest-assertion
```

```snapshot
error[PT009]: Use a regular `assert` instead of unittest-style `assertNotRegex`
 --> src/mdtest_snippet.py:5:9
  |
5 |         self.assertNotRegex("abc", "z")  # snapshot: pytest-unittest-assertion
  |         ^^^^^^^^^^^^^^^^^^^
help: Replace `assertNotRegex(...)` with `assert ...`
  |
4 |     def check(self):
  -         self.assertNotRegex("abc", "z")  # snapshot: pytest-unittest-assertion
5 +         assert not regex.search("z", "abc")  # snapshot: pytest-unittest-assertion
  |
note: This is an unsafe fix and may change runtime behavior
```

## Existing function alias

```py
from re import search as find

class Test:
    def check(self):
        self.assertRegex("abc", "a")  # snapshot: pytest-unittest-assertion
```

```snapshot
error[PT009]: Use a regular `assert` instead of unittest-style `assertRegex`
 --> src/mdtest_snippet.py:5:9
  |
5 |         self.assertRegex("abc", "a")  # snapshot: pytest-unittest-assertion
  |         ^^^^^^^^^^^^^^^^
help: Replace `assertRegex(...)` with `assert ...`
  |
4 |     def check(self):
  -         self.assertRegex("abc", "a")  # snapshot: pytest-unittest-assertion
5 +         assert find("a", "abc")  # snapshot: pytest-unittest-assertion
  |
note: This is an unsafe fix and may change runtime behavior
```

## Extend an existing import

```py
from re import compile

class Test:
    def check(self):
        self.assertRegex("abc", "a")  # snapshot: pytest-unittest-assertion
```

```snapshot
error[PT009]: Use a regular `assert` instead of unittest-style `assertRegex`
 --> src/mdtest_snippet.py:5:9
  |
5 |         self.assertRegex("abc", "a")  # snapshot: pytest-unittest-assertion
  |         ^^^^^^^^^^^^^^^^
help: Replace `assertRegex(...)` with `assert ...`
  |
  - from re import compile
1 + from re import compile, search
2 |
3 | class Test:
4 |     def check(self):
  -         self.assertRegex("abc", "a")  # snapshot: pytest-unittest-assertion
5 +         assert search("a", "abc")  # snapshot: pytest-unittest-assertion
  |
note: This is an unsafe fix and may change runtime behavior
```

## Keyword message

```py
class Test:
    def check(self):
        self.assertRegex("abc", "a", msg="expected a match")  # snapshot: pytest-unittest-assertion
```

```snapshot
error[PT009]: Use a regular `assert` instead of unittest-style `assertRegex`
 --> src/mdtest_snippet.py:3:9
  |
3 |         self.assertRegex("abc", "a", msg="expected a match")  # snapshot: pytest-unittest-assertion
  |         ^^^^^^^^^^^^^^^^
help: Replace `assertRegex(...)` with `assert ...`
  |
1 + import re
2 | class Test:
3 |     def check(self):
  -         self.assertRegex("abc", "a", msg="expected a match")  # snapshot: pytest-unittest-assertion
4 +         assert re.search("a", "abc"), "expected a match"  # snapshot: pytest-unittest-assertion
  |
note: This is an unsafe fix and may change runtime behavior
```

## Conflicting module name

A diagnostic is still emitted when the `re` name is unavailable, but no fix is offered.

```py
class Test:
    def check(self, re):
        self.assertRegex("abc", "a")  # snapshot: pytest-unittest-assertion
```

```snapshot
error[PT009]: Use a regular `assert` instead of unittest-style `assertRegex`
 --> src/mdtest_snippet.py:3:9
  |
3 |         self.assertRegex("abc", "a")  # snapshot: pytest-unittest-assertion
  |         ^^^^^^^^^^^^^^^^
help: Replace `assertRegex(...)` with `assert ...`
```

## Unsupported arguments

Invalid or unpacked arguments do not introduce an import or a replacement.

```py
class Test:
    def check(self, arguments):
        self.assertRegex("abc")  # snapshot: pytest-unittest-assertion
        self.assertRegex(*arguments)  # error: [pytest-unittest-assertion]
```

```snapshot
error[PT009]: Use a regular `assert` instead of unittest-style `assertRegex`
 --> src/mdtest_snippet.py:3:9
  |
3 |         self.assertRegex("abc")  # snapshot: pytest-unittest-assertion
  |         ^^^^^^^^^^^^^^^^
help: Replace `assertRegex(...)` with `assert ...`
```

## Preserve reused imports during unused-import fixes

The import edit prevents another fix from removing the import while the assertion starts using it.

```toml
[lint]
select = ["PT009", "F401"]
```

```py
import re as regex  # error: [unused-import]

class Test:
    def check(self):
        self.assertRegex("abc", "a")  # snapshot: pytest-unittest-assertion
```

```snapshot
error[PT009]: Use a regular `assert` instead of unittest-style `assertRegex`
 --> src/mdtest_snippet.py:5:9
  |
5 |         self.assertRegex("abc", "a")  # snapshot: pytest-unittest-assertion
  |         ^^^^^^^^^^^^^^^^
help: Replace `assertRegex(...)` with `assert ...`
  |
4 |     def check(self):
  -         self.assertRegex("abc", "a")  # snapshot: pytest-unittest-assertion
5 +         assert regex.search("a", "abc")  # snapshot: pytest-unittest-assertion
  |
note: This is an unsafe fix and may change runtime behavior
```
