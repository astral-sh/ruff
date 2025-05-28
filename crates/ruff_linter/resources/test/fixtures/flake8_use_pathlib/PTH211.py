import os
from pathlib import Path


os.symlink("usr/bin/python", "tmp/python")
os.symlink(b"usr/bin/python", b"tmp/python")
Path("tmp/python").symlink_to("usr/bin/python")  # Ok

os.symlink("usr/bin/python", "tmp/python", target_is_directory=True)
os.symlink(b"usr/bin/python", b"tmp/python", target_is_directory=True)
Path("tmp/python").symlink_to("usr/bin/python", target_is_directory=True)  # Ok
