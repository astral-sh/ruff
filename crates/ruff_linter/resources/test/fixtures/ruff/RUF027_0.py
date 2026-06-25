val = 2

"always ignore this: {val}"

print("but don't ignore this: {val}")  # RUF027


def simple_cases():
    a = 4
    b = "{a}"  # RUF027
    c = "{a} {b} f'{val}' "  # RUF027


def escaped_string():
    a = 4
    b = "escaped string: {{ brackets surround me }}"  # RUF027


def raw_string():
    a = 4
    b = r"raw string with formatting: {a}"  # RUF027
    c = r"raw string with \backslashes\ and \"escaped quotes\": {a}"  # RUF027


def print_name(name: str):
    a = 4
    print("Hello, {name}!")  # RUF027
    print("The test value we're using today is {a}")  # RUF027


def nested_funcs():
    a = 4
    print(do_nothing(do_nothing("{a}")))  # RUF027


def tripled_quoted():
    a = 4
    c = a
    single_line = """ {a} """  # RUF027
    # RUF027
    multi_line = a = """b { # comment
    c}  d
    """


def single_quoted_multi_line():
    a = 4
    # RUF027
    b = " {\
    a} \
    "


def implicit_concat():
    a = 4
    b = "{a}" "+" "{b}" r" \\ "  # RUF027 for the first part only
    print(f"{a}" "{a}" f"{b}")  # RUF027


def escaped_chars():
    a = 4
    b = "\"not escaped:\" '{a}' \"escaped:\": '{{c}}'"  # RUF027


def method_calls():
    value = {}
    value.method = print_name
    first = "Wendy"
    last = "Appleseed"
    value.method("{first} {last}")  # RUF027

def format_specifiers():
    a = 4
    b = "{a:b} {a:^5}"

# fstrings are never correct as type definitions
# so we should always skip those
def in_type_def():
    from typing import cast
    a = 'int'
    cast('f"{a}"','11')

# Regression test for parser bug
# https://github.com/astral-sh/ruff/issues/18860
def fuzz_bug():
    c('{\t"i}')

# Test case for backslash handling in f-string interpolations
# Should not trigger RUF027 for Python < 3.12 due to backslashes in interpolations
def backslash_test():
    x = "test"
    print("Hello {'\\n'}{x}")  # Should not trigger RUF027 for Python < 3.12

# Test case for comment handling in f-string interpolations
# Should not trigger RUF027 for Python < 3.12 due to comments in interpolations
def comment_test():
    x = "!"
    print("""{x  # }
}""")

# Test case for `#` inside a nested string literal in interpolation
# `#` inside a string is NOT a comment — should trigger RUF027 even on Python < 3.12
def hash_in_string_test():
    x = "world"
    print("Hello {'#'}{x}")   # RUF027: `#` is inside a string, not a comment
    print("Hello {\"#\"}{x}") # RUF027: same, double-quoted

# Test case for `#` in format spec (e.g., `{1:#x}`)
# `#` in a format spec is NOT a comment — should trigger RUF027 even on Python < 3.12
def hash_in_format_spec_test():
    n = 255
    print("Hex: {n:#x}")   # RUF027: `#` is in format spec, not a comment
    print("Oct: {n:#o}")   # RUF027: same

# Test case for `#` in nested interpolation inside format spec (e.g., `{1:{x #}}`)
# The `#` is a comment inside a nested interpolation — should NOT trigger RUF027 on Python < 3.12
def hash_in_nested_format_spec_test():
    x = 5
    print("""{1:{x #}}
}""")  # Should not trigger RUF027 for Python < 3.12
