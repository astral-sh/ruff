# Empty t-strings
t""
t""
t''
t""""""
t''''''

t"{" t"}"
t"{foo!s}"
t"{3,}"
t"{3!=4:}"
t'{3:{"}"}>10}'
t'{3:{"{"}>10}'
t"{  foo =  }"
t"{  foo =  :.3f  }"
t"{  foo =  !s  }"
t"{  1, 2  =  }"
t'{t"{3.1415=:.1f}":*^20}'

{"foo " t"bar {x + y} " "baz": 10}
match foo:
    case "one":
        pass
    case "implicitly " "concatenated":
        pass

t"\{foo}\{bar:\}"
t"\\{{foo\\}}"
t"""{
    foo:x
        y
        z
}"""
t"{ (  foo )  = }"

t"normal {foo} {{another}} {bar} {{{three}}}"
t"normal {foo!a} {bar!s} {baz!r} {foobar}"
t"normal {x:y + 2}"
t"{x:{{1}.pop()}}"
t"{(lambda x:{x})}"
t"{x =}"
t"{    x = }"
t"{x=!a}"
t"{x:.3f!r =}"
t"{x = !r :.3f}"
t"{x:.3f=!r}"
"hello" t"{x}"
t"{x}" t"{y}"
t"{x}" "world"
t"Invalid args in command: {command, *args}"
"foo" t"{x}" "bar"
(
    t"a"
    t"b"
    "c"
    rt"d"
    fr"e"
)

# With unicode strings
u"foo" t"{bar}" "baz" " some"
"foo" t"{bar}" u"baz" " some"
"foo" t"{bar}" "baz" u" some"
u"foo" t"bar {baz} really" u"bar" "no"


# With f-strings
f"{this}" t"{that}"
t"{this}"f"{that}"
t"{this}" "that" f"{other}"
f"one {this} two" "that" t"three {other} four"

# Nesting
t"{f"{t"{this}"}"}"
