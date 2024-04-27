num = 1337

def return_num() -> int:
    return num

print(oct(num)[2:])  # FURB116
print(hex(num)[2:])  # FURB116
print(bin(num)[2:])  # FURB116

print(oct(1337)[2:])  # FURB116
print(hex(1337)[2:])  # FURB116
print(bin(1337)[2:])  # FURB116

print(bin(return_num())[2:])  # FURB116 (no autofix)
print(bin(int(f"{num}"))[2:])  # FURB116 (no autofix)

## invalid
print(oct(0o1337)[1:])
print(hex(0x1337)[3:])
