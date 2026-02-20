import os
import stat

keyfile = "foo"

os.chmod("/etc/passwd", 0o227)  # Error
os.chmod("/etc/passwd", 0o7)  # Error
os.chmod("/etc/passwd", 0o664)  # Error
os.chmod("/etc/passwd", 0o777)  # Error
os.chmod("/etc/passwd", 0o770)  # Error
os.chmod("/etc/passwd", 0o776)  # Error
os.chmod("/etc/passwd", 0o760)  # Error
os.chmod("~/.bashrc", 511)  # Error
os.chmod("/etc/hosts", 0o777)  # Error
os.chmod("/tmp/oh_hai", 0x1FF)  # Error
os.chmod("/etc/passwd", stat.S_IRWXU)  # OK
os.chmod(keyfile, 0o777)  # Error
os.chmod(keyfile, 0o7 | 0o70 | 0o700)  # Error
os.chmod(keyfile, stat.S_IRWXO | stat.S_IRWXG | stat.S_IRWXU)  # Error
os.chmod("~/hidden_exec", stat.S_IXGRP)  # Error
os.chmod("~/hidden_exec", stat.S_IXOTH)  # Error
os.chmod("/etc/passwd", stat.S_IWOTH)  # Error
os.chmod("/etc/passwd", stat.S_IWGRP)  # Error
os.chmod("/etc/passwd", 0o1000)  # OK
os.chmod("/etc/passwd", 0o777777)  # Error
os.chmod("/etc/passwd", 0o100000000)  # Error
os.chmod("/etc/secrets.txt", 0o21)  # Error

mode = 0o700
os.chmod("/etc/passwd", mode | 0o777)  # Error
os.chmod("/etc/passwd", 0o777777 & 0o700)  # OK
os.chmod("/etc/passwd", 0o777777 & 0o777)  # Error
