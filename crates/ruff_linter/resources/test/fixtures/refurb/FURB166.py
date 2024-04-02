# Errors

_ = int("0b1010"[2:], 2)
_ = int("0o777"[2:], 8)
_ = int("0xFFFF"[2:], 16)

b = "0b11"
_ = int(b[2:], 2)

_ = int("0xFFFF"[2:], base=16)

_ = int(b"0xFFFF"[2:], 16)


def get_str():
    return "0xFFF"


_ = int(get_str()[2:], 16)

# OK

_ = int("0b1100", 0)
_ = int("123", 3)
_ = int("123", 10)
_ = int("0b1010"[3:], 2)
_ = int("0b1010"[:2], 2)
_ = int("12345"[2:])
_ = int("12345"[2:], xyz=1)  # type: ignore
