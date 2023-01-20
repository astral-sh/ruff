# These SHOULD change
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

"brace {} %s" % (1,)

"%s" % (
    "trailing comma",
        )

paren_continue = (
    "foo %s "
    "bar %s" % (x, y)
)

print("foo %s " % (x,))

paren_string = (
    "foo %s "
    "bar %s"
) % (x, y)

# Should issue warning but no change
"%s \N{snowman}" % (a,)

# These should NOT change
"%s" % unknown_type

b"%s" % (b"bytestring",)

"%*s" % (5, "hi")

"%d" % (flt,)

"%c" % (some_string,)

"%#o" % (123,)

"%4%" % ()

"%.2r" % (1.25)

i % 3

"%.*s" % (5, "hi")

"%i" % (flt,)

pytest.param('"%8s" % (None,)', id='unsafe width-string conversion'),
