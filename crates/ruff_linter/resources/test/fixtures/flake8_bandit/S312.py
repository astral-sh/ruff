from telnetlib import Telnet

Telnet("localhost", 23)


# https://github.com/astral-sh/ruff/issues/15522
map(Telnet, [])
foo = Telnet

import telnetlib
_ = telnetlib.Telnet

def _() -> Telnet: ...
