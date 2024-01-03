import ssl
from ssl import wrap_socket


ssl.wrap_socket()  # S504
wrap_socket()  # S504
ssl.wrap_socket(ssl_version=ssl.PROTOCOL_TLSv1_2)  # OK


class Foo:
    def wrap_socket(self):
        pass


f = Foo()
f.wrap_socket()  # OK
