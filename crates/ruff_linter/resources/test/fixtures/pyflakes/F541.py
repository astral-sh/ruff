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
f"{{test}}"
f'{{ 40 }}'
f"{{a {{x}}"
f"{{{{x}}}}"
""f""
''f""
(""f""r"")
f"{v:{f"0.2f"}}"
f"\{{x}}"


# Docstring position: f-string removal would create a docstring, so fix
# should be suppressed (diagnostic still emitted, but no fix attached).
def docstring_func():
    f"This would become a docstring"
    pass


class DocstringClass:
    f"This would become a class docstring"
    pass


# Non-docstring position: fix should still be applied.
def non_docstring_func():
    pass
    f"This is not in docstring position"


class NonDocstringClass:
    x = 1
    f"This is not in docstring position either"
