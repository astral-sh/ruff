# ASCII literals should be replaced by a bytes literal
"foo".encode("utf-8")  # b"foo"
"foo".encode("u8")  # b"foo"
"foo".encode()  # b"foo"
"foo".encode("UTF8")  # b"foo"
U"foo".encode("utf-8")  # b"foo"
"foo".encode(encoding="utf-8")  # b"foo"
"""
Lorem

Ipsum
""".encode(
    "utf-8"
)
# b"""
# Lorem
#
# Ipsum
# """

# `encode` on variables should not be processed.
string = "hello there"
string.encode("utf-8")

bar = "bar"
f"foo{bar}".encode("utf-8")  # f"foo{bar}".encode()
encoding = "latin"
"foo".encode(encoding)
f"foo{bar}".encode(encoding)

# `encode` with custom args and kwargs should not be processed.
"foo".encode("utf-8", errors="replace")
"foo".encode("utf-8", "replace")
"foo".encode(errors="replace")
"foo".encode(encoding="utf-8", errors="replace")

# `encode` with custom args and kwargs on unicode should not be processed.
"unicode text©".encode("utf-8", errors="replace")
"unicode text©".encode("utf-8", "replace")
"unicode text©".encode(errors="replace")
"unicode text©".encode(encoding="utf-8", errors="replace")

# Unicode literals should only be stripped of default encoding.
"unicode text©".encode("utf-8")  # "unicode text©".encode()
"unicode text©".encode()
"unicode text©".encode(encoding="UTF8")  # "unicode text©".encode()

r"foo\o".encode("utf-8")  # br"foo\o"
u"foo".encode("utf-8")  # b"foo"
R"foo\o".encode("utf-8")  # br"foo\o"
U"foo".encode("utf-8")  # b"foo"
print("foo".encode())  # print(b"foo")
