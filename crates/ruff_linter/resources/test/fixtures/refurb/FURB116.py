import datetime
import sys

num = 1337

def return_num() -> int:
    return num

print(oct(num)[2:])  # FURB116
print(hex(num)[2:])  # FURB116
print(bin(num)[2:])  # FURB116

print(oct(1337)[2:])  # FURB116
print(hex(1337)[2:])  # FURB116
print(bin(1337)[2:])  # FURB116
print(bin(+1337)[2:])  # FURB116

print(bin(return_num())[2:])  # FURB116 (no autofix)
print(bin(int(f"{num}"))[2:])  # FURB116 (no autofix)

## invalid
print(oct(0o1337)[1:])
print(hex(0x1337)[3:])

# https://github.com/astral-sh/ruff/issues/16472
# float and complex numbers should be ignored
print(bin(1.0)[2:])
print(bin(3.14j)[2:])

d = datetime.datetime.now(tz=datetime.UTC)
# autofix is display-only
print(bin(d)[2:])
# no autofix for Python 3.11 and earlier, as it introduces a syntax error
print(bin(len("xyz").numerator)[2:])

# autofix is display-only
print(bin({0: 1}[0].numerator)[2:])
# no autofix for Python 3.11 and earlier, as it introduces a syntax error
print(bin(ord("\\").numerator)[2:])
print(hex(sys
.maxunicode)[2:])

# for negatives numbers autofix is display-only
print(bin(-1)[2:])
