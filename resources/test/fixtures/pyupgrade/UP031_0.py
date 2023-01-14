# These SHOULD change
'%s %s' % (a, b)

"trivial" % ()

"%s" % ("simple",)

# By itself this works fine, but when with certain other ones it breaks
"%s" % ("%s" % ("nested",),)

"%s%% percent" % (15,)

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
"%s \N{snowman}" % (a,)
# Make sure to include assignment and inside a call, also multi-line

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
