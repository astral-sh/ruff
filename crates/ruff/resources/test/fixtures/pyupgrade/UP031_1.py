# OK
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

pytest.param('"%8s" % (None,)', id="unsafe width-string conversion"),

"%()s" % {"": "bar"}

"%(1)s" % {1: 2, "1": 2}

"%(and)s" % {"and": 2}

# OK (arguably false negatives)
'Hello %s' % bar

'Hello %s' % bar.baz

'Hello %s' % bar['bop']
