# Test suite from Refurb
# See https://github.com/dosisod/refurb/blob/db02242b142285e615a664a8d3324470bb711306/test/data/err_188.py

# these should match

def remove_extension_via_slice(filename: str) -> str:
    if filename.endswith(".txt"):
        filename = filename[:-4]

    return filename


def remove_extension_via_slice_len(filename: str, extension: str) -> str:
    if filename.endswith(extension):
        filename = filename[:-len(extension)]

    return filename


def remove_extension_via_ternary(filename: str) -> str:
    return filename[:-4] if filename.endswith(".txt") else filename


def remove_extension_via_ternary_with_len(filename: str, extension: str) -> str:
    return filename[:-len(extension)] if filename.endswith(extension) else filename


def remove_prefix(filename: str) -> str:
    return filename[4:] if filename.startswith("abc-") else filename


def remove_prefix_via_len(filename: str, prefix: str) -> str:
    return filename[len(prefix):] if filename.startswith(prefix) else filename


# these should not

def remove_extension_with_mismatched_len(filename: str) -> str:
    if filename.endswith(".txt"):
        filename = filename[:3]

    return filename


def remove_extension_assign_to_different_var(filename: str) -> str:
    if filename.endswith(".txt"):
        other_var = filename[:-4]

    return filename


def remove_extension_with_multiple_stmts(filename: str) -> str:
    if filename.endswith(".txt"):
        print("do some work")

        filename = filename[:-4]

    if filename.endswith(".txt"):
        filename = filename[:-4]

        print("do some work")

    return filename


def remove_extension_from_unrelated_var(filename: str) -> str:
    xyz = "abc.txt"

    if filename.endswith(".txt"):
        filename = xyz[:-4]

    return filename


def remove_extension_in_elif(filename: str) -> str:
    if filename:
        pass

    elif filename.endswith(".txt"):
        filename = filename[:-4]

    return filename


def remove_extension_in_multiple_elif(filename: str) -> str:
    if filename:
        pass

    elif filename:
        pass

    elif filename.endswith(".txt"):
        filename = filename[:-4]

    return filename


def remove_extension_in_if_with_else(filename: str) -> str:
    if filename.endswith(".txt"):
        filename = filename[:-4]

    else:
        pass

    return filename


def remove_extension_ternary_name_mismatch(filename: str):
    xyz = ""

    _ = xyz[:-4] if filename.endswith(".txt") else filename
    _ = filename[:-4] if xyz.endswith(".txt") else filename
    _ = filename[:-4] if filename.endswith(".txt") else xyz


def remove_extension_slice_amount_mismatch(filename: str) -> None:
    extension = ".txt"

    _ = filename[:-1] if filename.endswith(".txt") else filename
    _ = filename[:-1] if filename.endswith(extension) else filename
    _ = filename[:-len("")] if filename.endswith(extension) else filename


def remove_prefix_size_mismatch(filename: str) -> str:
    return filename[3:] if filename.startswith("abc-") else filename


def remove_prefix_name_mismatch(filename: str) -> None:
    xyz = ""

    _ = xyz[4:] if filename.startswith("abc-") else filename
    _ = filename[4:] if xyz.startswith("abc-") else filename
    _ = filename[4:] if filename.startswith("abc-") else xyz

# ---- End of refurb test suite ---- #

# ---- Begin ruff specific test suite --- #

# these should be linted

def remove_suffix_multiple_attribute_expr() -> None:
    import foo.bar

    SUFFIX = "suffix"

    x = foo.bar.baz[:-len(SUFFIX)] if foo.bar.baz.endswith(SUFFIX) else foo.bar.baz

def remove_prefix_comparable_literal_expr() -> None:
    return ("abc" "def")[3:] if ("abc" "def").startswith("abc") else "abc" "def"

def shadow_builtins(filename: str, extension: str) -> None:
    from builtins import len as builtins_len

    return filename[:-builtins_len(extension)] if filename.endswith(extension) else filename

def okay_steps():
    text = "!x!y!z"
    if text.startswith("!"):
        text = text[1::1]
    if text.startswith("!"):
        text = text[1::True]
    if text.startswith("!"):
        text = text[1::None]
    print(text)


# this should be skipped
def ignore_step():
    text = "!x!y!z"
    if text.startswith("!"):
        text = text[1::2]
    print(text)

def handle_unicode():
    # should be skipped!
    text = "řetězec"
    if text.startswith("ř"): 
        text = text[2:]

    # should be linted
    # with fix `text = text.removeprefix("ř")`
    text = "řetězec"
    if text.startswith("ř"): 
        text = text[1:]


def handle_surrogates():
    # should be linted
    text = "\ud800\udc00heythere"
    if text.startswith("\ud800\udc00"):
        text = text[2:]
    text = "\U00010000heythere"
    if text.startswith("\U00010000"):
        text = text[1:]
    
    # should not be linted
    text = "\ud800\udc00heythere"
    if text.startswith("\ud800\udc00"):
        text = text[1:]

# Regression test for
# https://github.com/astral-sh/ruff/issues/16231
def func():
    a = "sjdfaskldjfakljklfoo"
    if a.endswith("foo"):
        a = a[: -len("foo")]
    print(a)
