import sys

if sys.platform == "linus": ...  # Error: PYI008 Unrecognized platform `linus`

if sys.platform != "linux": ...  # OK

if sys.platform == "win32": ...  # OK

if sys.platform != "darwin": ...  # OK

if sys.platform == "cygwin": ...  # OK
