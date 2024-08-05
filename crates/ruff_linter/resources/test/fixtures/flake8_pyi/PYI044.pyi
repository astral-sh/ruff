# Bad import.
from __future__ import annotations # PYI044.
from __future__ import annotations, OtherThing # PYI044.

# Good imports.
from __future__ import Something
import sys
from socket import AF_INET
