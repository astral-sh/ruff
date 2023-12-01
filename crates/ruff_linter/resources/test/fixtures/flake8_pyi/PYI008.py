import sys

if sys.platform == "linus": ...  # OK

if sys.platform != "linux": ...  # OK

if sys.platform == "win32": ...  # OK

if sys.platform != "darwin": ...  # OK

if sys.platform == "cygwin": ...  # OK
