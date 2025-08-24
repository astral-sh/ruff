# These should not change
raise ValueError
raise ValueError(1)

from .mmap import error
raise error

# Testing the modules
import socket, mmap, select, resource
raise socket.error
raise mmap.error
raise select.error
raise resource.error

raise socket.error()
raise mmap.error(1)
raise select.error(1, 2)
raise resource.error(1, "strerror", "filename")

raise socket.error(
    1,
    2,
    3,
)

from mmap import error
raise error

from socket import error
raise error(1)

from select import error
raise error(1, 2)

from resource import error
raise error(1, "strerror", "filename")

# Testing the names
raise EnvironmentError
raise IOError
raise WindowsError

raise EnvironmentError()
raise IOError(1)
raise WindowsError(1, 2)

raise EnvironmentError(
    1,
    2,
    3,
)

raise WindowsError
raise EnvironmentError(1)
raise IOError(1, 2)
