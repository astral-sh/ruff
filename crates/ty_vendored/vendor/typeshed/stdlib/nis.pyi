"""This module contains functions for accessing NIS maps."""

import sys

if sys.platform != "win32":
    def cat(map: str, domain: str = ...) -> dict[str, str]:
        """cat(map, domain = defaultdomain)
        Returns the entire map as a dictionary. Optionally domain can be
        specified but it defaults to the system default domain.
        """

    def get_default_domain() -> str:
        """get_default_domain() -> str
        Corresponds to the C library yp_get_default_domain() call, returning
        the default NIS domain.
        """

    def maps(domain: str = ...) -> list[str]:
        """maps(domain = defaultdomain)
        Returns an array of all available NIS maps within a domain. If domain
        is not specified it defaults to the system default domain.
        """

    def match(key: str, map: str, domain: str = ...) -> str:
        """match(key, map, domain = defaultdomain)
        Corresponds to the C library yp_match() call, returning the value of
        key in the given map. Optionally domain can be specified but it
        defaults to the system default domain.
        """

    class error(Exception): ...
