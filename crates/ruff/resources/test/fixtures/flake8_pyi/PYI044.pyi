# Bad import.
from __future__ import Something # PYI044, since this imports from __future__.

# Good imports.
import sys
from socket import AF_INET
