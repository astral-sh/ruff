field01: int = 0xFFFFFFFF
field02: int = 0xFFFFFFFFF  # Error: PYI054
field03: int = -0xFFFFFFFF
field04: int = -0xFFFFFFFFF  # Error: PYI054

field05: int = 1234567890
field06: int = 12_456_890
field07: int = 12345678901  # Error: PYI054
field08: int = -1234567801
field09: int = -234_567_890  # Error: PYI054

field10: float = 123.456789
field11: float = 123.4567890  # Error: PYI054
field12: float = -123.456789
field13: float = -123.567_890  # Error: PYI054

field14: complex = 1e1234567j
field15: complex = 1e12345678j  # Error: PYI054
field16: complex = -1e1234567j
field17: complex = 1e123456789j  # Error: PYI054
