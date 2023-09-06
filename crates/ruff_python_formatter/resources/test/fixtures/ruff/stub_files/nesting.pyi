"""Tests specifically for https://github.com/psf/black/issues/3861"""

import sys


class OuterClassOrOtherSuite:
    class Nested11:
        class Nested12:
            assignment = 1
            def function_definition(self): ...

    def f1(self) -> str: ...

    class Nested21:
        class Nested22:
            def function_definition(self): ...
            assignment = 1

    def f2(self) -> str: ...

if sys.version_info > (3, 7):
    if sys.platform == "win32":
        assignment = 1
        def function_definition(self): ...

    def f1(self) -> str: ...
    if sys.platform != "win32":
        def function_definition(self): ...
        assignment = 1

    def f2(self) -> str: ...
