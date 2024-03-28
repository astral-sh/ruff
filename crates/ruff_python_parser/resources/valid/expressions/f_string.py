# Empty f-strings
f""
F""
f''
f""""""
f''''''

f"{" f"}"
f"{foo!s}"
f"{3,}"
f"{3!=4:}"
f'{3:{"}"}>10}'
f'{3:{"{"}>10}'
f"{  foo =  }"
f"{  foo =  :.3f  }"
f"{  foo =  !s  }"
f"{  1, 2  =  }"
f'{f"{3.1415=:.1f}":*^20}'

{"foo " f"bar {x + y} " "baz": 10}
match foo:
    case "one":
        pass
    case "implicitly " "concatenated":
        pass

f"\{foo}\{bar:\}"
f"\\{{foo\\}}"
f"""{
    foo:x
        y
        z
}"""
f"{ (  foo )  = }"

f"normal {foo} {{another}} {bar} {{{three}}}"
f"normal {foo!a} {bar!s} {baz!r} {foobar}"
f"normal {x:y + 2}"
f"{x:{{1}.pop()}}"
f"{(lambda x:{x})}"
f"{x =}"
f"{    x = }"
f"{x=!a}"
f"{x:.3f!r =}"
f"{x = !r :.3f}"
f"{x:.3f=!r}"
"hello" f"{x}"
f"{x}" f"{y}"
f"{x}" "world"
f"Invalid args in command: {command, *args}"
"foo" f"{x}" "bar"
(
    f"a"
    F"b"
    "c"
    rf"d"
    fr"e"
)

# With unicode strings
u"foo" f"{bar}" "baz" " some"
"foo" f"{bar}" u"baz" " some"
"foo" f"{bar}" "baz" u" some"
u"foo" f"bar {baz} really" u"bar" "no"
