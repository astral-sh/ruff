import mmap, socket, select

try:
    pass
except (OSError, mmap.error, IOError):
    pass
except (OSError, socket.error, KeyError):
    pass

try:
    pass
except (
    OSError,
    select.error,
    IOError,
):
    pass
