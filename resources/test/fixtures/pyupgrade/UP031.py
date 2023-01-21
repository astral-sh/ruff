# UP031
'%s %s' % (a, b)

"trivial" % ()

"%s" % ("simple",)

"%s" % ("%s" % ("nested",),)

"%s%% percent" % (15,)

"%f" % (15,)

"%.f" % (15,)

"%.3f" % (15,)

"%3f" % (15,)

"%-5f" % (5,)

"%9f" % (5,)

"%#o" % (123,)

"brace {} %s" % (1,)

"%s" % (
    "trailing comma",
        )

print("foo %s " % (x,))

"%()s" % {"": "bar"}

"%(1)s" % {1: 2, "1": 2}

"%(and)s" % {"and": 2}

"%(ab)s" % {"a" "b": 1}

"%(a)s" % {"a"  :  1}

"%(k)s" % {"k": "v"}

"%(k)s" % {
    "k": "v",
    "i": "j"
}

"%(to_list)s" % {"to_list": []}

"%(k)s" % {"k": "v", "i": 1, "j": []}

paren_continue = (
    "foo %s "
    "bar %s" % (x, y)
)

paren_string = (
    "foo %s "
    "bar %s"
) % (x, y)

paren_continue = (
    "foo %(foo)s "
    "bar %(bar)s" % {"foo": x, "bar": y}
)

paren_string = (
    "foo %(foo)s "
    "bar %(bar)s"
) % {"foo": x, "bar": y}

# UP031 (without fix)
"%s \N{snowman}" % (a,)

"%(foo)s \N{snowman}" % {"foo": 1}

# OK
"%s" % unknown_type

b"%s" % (b"bytestring",)

"%*s" % (5, "hi")

"%d" % (flt,)

"%c" % (some_string,)

"%4%" % ()

"%.2r" % (1.25)

i % 3

"%.*s" % (5, "hi")

"%i" % (flt,)

"%()s" % {"": "empty"}

"%s" % {"k": "v"}

"%(1)s" % {"1": "bar"}

"%(a)s" % {"a": 1, "a": 2}

pytest.param('"%8s" % (None,)', id='unsafe width-string conversion'),
