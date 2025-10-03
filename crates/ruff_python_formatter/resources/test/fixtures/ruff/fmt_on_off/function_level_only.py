# Function-level fmt: off should not suppress module-level trailing newline
def test():
    # fmt: off
    x=1
    # fmt: on
    return x

print("module")