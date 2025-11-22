"""RUF066 - Single-assignment missing Final.

Should NOT warn - The variable is already annotated with typing.Final.
"""

from typing import Final

X: Final = 1
print(X)
