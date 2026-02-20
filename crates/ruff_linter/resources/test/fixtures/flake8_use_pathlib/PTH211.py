import os
from pathlib import Path


os.symlink("usr/bin/python", "tmp/python")
os.symlink(b"usr/bin/python", b"tmp/python")
Path("tmp/python").symlink_to("usr/bin/python")  # Ok

os.symlink("usr/bin/python", "tmp/python", target_is_directory=True)
os.symlink(b"usr/bin/python", b"tmp/python", target_is_directory=True)
Path("tmp/python").symlink_to("usr/bin/python", target_is_directory=True)  # Ok

fd = os.open(".", os.O_RDONLY)
os.symlink("source.txt", "link.txt", dir_fd=fd)  # Ok: dir_fd is not supported by pathlib
os.close(fd)

os.symlink(src="usr/bin/python", dst="tmp/python", unknown=True)
os.symlink("usr/bin/python",  dst="tmp/python", target_is_directory=False)

os.symlink(src="usr/bin/python", dst="tmp/python", dir_fd=None)

os.symlink("usr/bin/python",  dst="tmp/python", target_is_directory=     True    )
os.symlink("usr/bin/python",  dst="tmp/python", target_is_directory="nonboolean")
