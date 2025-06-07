import os
from pathlib import Path

os.chmod("foo", 444)  # Error
os.chmod("foo", 0o444)  # OK
os.chmod("foo", 7777)  # Error
os.chmod("foo", 10000)  # Error
os.chmod("foo", 99999)  # Error
Path("bar").chmod(755)  # Error
Path("bar").chmod(0o755)  # OK
path = Path("bar")
path.chmod(755)  # Error
path.chmod(0o755)  # OK
