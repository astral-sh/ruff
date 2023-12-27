x = 10

def ten() -> int:
    return 10

count = bin(x).count("1")  # FURB161
count = bin(10).count("1")  # FURB161
count = bin(0b1010).count("1")  # FURB161
count = bin(0xA).count("1")  # FURB161
count = bin(0o12).count("1")  # FURB161
count = bin(0x10 + 0x1000).count("1")  # FURB161
count = bin(ten()).count("1")  # FURB161
count = bin((10)).count("1")  # FURB161
count = bin("10" "15").count("1")  # FURB161

count = x.bit_count()  # OK
count = (10).bit_count()  # OK
count = 0b1010.bit_count()  # OK
count = 0xA.bit_count()  # OK
count = 0o12.bit_count()  # OK
count = (0x10 + 0x1000).bit_count()  # OK
count = ten().bit_count()  # OK
