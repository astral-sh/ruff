# These SHOULD change
'%s %s' % (a, b)

"trivial" % ()

"%s" % ("simple",)

"%s" % ("%s" % ("nested",),)

"%s%% percent" % (15,)

"%3f" % (15,)

"%-5f" % (5,)

"%9f" % (5,)

"brace {} %s" % (1,)

"%s" % (
    "trailing comma",
        )

"%s \N{snowman}" % (a,)
# Make sure to include assignment and inside a call, also multi-line
