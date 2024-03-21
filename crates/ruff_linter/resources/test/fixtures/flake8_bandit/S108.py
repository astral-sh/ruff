# ok
with open("/abc/tmp", "w") as f:
    f.write("def")

with open("/tmp/abc", "w") as f:
    f.write("def")

with open(f"/tmp/abc", "w") as f:
    f.write("def")

with open("/var/tmp/123", "w") as f:
    f.write("def")

with open("/dev/shm/unit/test", "w") as f:
    f.write("def")

# not ok by config
with open("/foo/bar", "w") as f:
    f.write("def")

# Implicit string concatenation
with open("/tmp/" "abc", "w") as f:
    f.write("def")

with open("/tmp/abc" f"/tmp/abc", "w") as f:
    f.write("def")

# Using `tempfile` module should be ok
import tempfile
from tempfile import TemporaryDirectory

with tempfile.NamedTemporaryFile(dir="/tmp") as f:
    f.write(b"def")

with tempfile.NamedTemporaryFile(dir="/var/tmp") as f:
    f.write(b"def")

with tempfile.TemporaryDirectory(dir="/dev/shm") as d:
    pass

with TemporaryDirectory(dir="/tmp") as d:
    pass
