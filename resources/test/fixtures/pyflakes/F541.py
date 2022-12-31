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

# OK
g = f"ghi{123:{45}}"

# Error
h = "x" "y" f"z"

v = 23.234234

# OK
f"{v:0.2f}"

# OK (false negatives)
f"{v:{f'0.2f'}}"
f"{f''}"
