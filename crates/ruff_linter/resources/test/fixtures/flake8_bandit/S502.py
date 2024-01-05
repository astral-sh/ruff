import ssl
from ssl import wrap_socket
from OpenSSL import SSL
from OpenSSL.SSL import Context

wrap_socket(ssl_version=ssl.PROTOCOL_SSLv3)  # S502
ssl.wrap_socket(ssl_version=ssl.PROTOCOL_TLSv1)  # S502
ssl.wrap_socket(ssl_version=ssl.PROTOCOL_SSLv2)  # S502
SSL.Context(method=SSL.SSLv2_METHOD)  # S502
SSL.Context(method=SSL.SSLv23_METHOD)  # S502
Context(method=SSL.SSLv3_METHOD)  # S502
Context(method=SSL.TLSv1_METHOD)  # S502

wrap_socket(ssl_version=ssl.PROTOCOL_TLS_CLIENT)  # OK
SSL.Context(method=SSL.TLS_SERVER_METHOD)  # OK
func(ssl_version=ssl.PROTOCOL_TLSv1_2)  # OK
