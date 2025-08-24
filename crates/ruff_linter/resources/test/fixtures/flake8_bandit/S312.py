from telnetlib import Telnet

Telnet("localhost", 23)


# https://github.com/astral-sh/ruff/issues/15522
map(Telnet, [])
foo = Telnet

import telnetlib
_ = telnetlib.Telnet

from typing import Annotated
foo: Annotated[Telnet, telnetlib.Telnet()]

def _() -> Telnet: ...
