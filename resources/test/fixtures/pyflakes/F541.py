# OK
a = "abc"
b = f"ghi{'jkl'}"

# Errors
c = f"def"
d = f"def" + "ghi"
e = (
    f"def" +
    "ghi"
)
f = (
    f"a"
    F"b"
    "c"
    rf"d"
    fr"e"
)
g = f""

# OK
g = f"ghi{123:{45}}"

# Error
h = "x" "y" f"z"

v = 23.234234

# OK
f"{v:0.2f}"
f"{f'{v:0.2f}'}"

# Errors
f"{v:{f'0.2f'}}"
f"{f''}"
