import dbm.gnu
import dbm.ndbm
import os
from pathlib import Path

os.chmod("foo", 444)  # Error
os.chmod("foo", 0o444)  # OK
os.chmod("foo", 7777)  # Error
os.chmod("foo", 10000)  # Error
os.chmod("foo", 99999)  # Error

os.umask(777)  # Error
os.umask(0o777)  # OK

os.fchmod(0, 400)  # Error
os.fchmod(0, 0o400)  # OK

os.lchmod("foo", 755)  # Error
os.lchmod("foo", 0o755)  # OK

os.mkdir("foo", 600)  # Error
os.mkdir("foo", 0o600)  # OK

os.makedirs("foo", 644)  # Error
os.makedirs("foo", 0o644)  # OK

os.mkfifo("foo", 640)  # Error
os.mkfifo("foo", 0o640)  # OK

os.mknod("foo", 660)  # Error
os.mknod("foo", 0o660)  # OK

os.open("foo", os.O_CREAT, 644)  # Error
os.open("foo", os.O_CREAT, 0o644)  # OK

Path("bar").chmod(755)  # Error
Path("bar").chmod(0o755)  # OK

path = Path("bar")
path.chmod(755)  # Error
path.chmod(0o755)  # OK

dbm.open("db", "r", 600)  # Error
dbm.open("db", "r", 0o600)  # OK

dbm.gnu.open("db", "r", 600)  # Error
dbm.gnu.open("db", "r", 0o600)  # OK

dbm.ndbm.open("db", "r", 600)  # Error
dbm.ndbm.open("db", "r", 0o600)  # OK

os.fchmod(0, 256)  # 0o400
os.fchmod(0, 493)  # 0o755

# https://github.com/astral-sh/ruff/issues/19010
os.chmod("foo", 000)  # Error
os.chmod("foo", 0000)  # Error

os.chmod("foo", 0b0)  # Error
os.chmod("foo", 0x0)  # Error
os.chmod("foo", 0)  # Ok
