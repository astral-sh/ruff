# These SHOULD change
'%s %s' % (a, b)

"trivial" % ()

"%s" % ("simple",)

# Breaks based on other things in the file
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
""" Having this uncommented breaks the nested one, waiting on help from Charlie to uncomment this
"%s \N{snowman}" % (a,)
"""
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
