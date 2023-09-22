import sys
import tempfile

print("Hello, world!")  # T201
print("Hello, world!", file=None)  # T201
print("Hello, world!", file=sys.stdout)  # T201
print("Hello, world!", file=sys.stderr)  # T201

with tempfile.NamedTemporaryFile() as fp:
    print("Hello, world!", file=fp)  # OK
