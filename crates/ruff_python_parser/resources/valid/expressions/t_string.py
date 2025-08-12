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

{t"foo " t"bar {x + y} " t"baz": 10}
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
t"hello" t"{x}"
t"{x}" t"{y}"
t"{x}" t"world"
t"Invalid args in command: {command, *args}"
t"foo" t"{x}" t"bar"
(
    t"a"
    t"b"
    t"c"
    rt"d"
    tr"e"
)

# Nesting
t"{f"{t"{this}"}"}"
