class SocketError(Exception):
    pass


try:
    raise SocketError()
except SocketError:
    pass
