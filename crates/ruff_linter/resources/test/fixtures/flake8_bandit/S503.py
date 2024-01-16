import ssl
from OpenSSL import SSL
from ssl import PROTOCOL_TLSv1


def func(version=ssl.PROTOCOL_SSLv2):  # S503
    pass


def func(protocol=SSL.SSLv2_METHOD):  # S503
    pass


def func(version=SSL.SSLv23_METHOD):  # S503
    pass


def func(protocol=PROTOCOL_TLSv1):  # S503
    pass


def func(version=SSL.TLSv1_2_METHOD):  # OK
    pass
