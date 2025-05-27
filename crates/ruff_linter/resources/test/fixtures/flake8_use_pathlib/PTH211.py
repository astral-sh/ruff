import os
from pathlib import Path


os.symlink("usr/bin/python", "tmp/python")
os.symlink(b"usr/bin/python", b"tmp/python")
Path("usr/bin/python").symlink_to("tmp/python")

os.symlink("usr/bin/python", "tmp/python", target_is_directory=True)
os.symlink(b"usr/bin/python", b"tmp/python", target_is_directory=True)
Path("usr/bin/python").symlink_to("tmp/python")
