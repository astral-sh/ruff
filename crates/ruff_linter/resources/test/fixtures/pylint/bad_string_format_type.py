# Errors
print("foo %(foo)d bar %(bar)d" % {"foo": "1", "bar": "2"})

"foo %e bar %s" % ("1", 2)

"%d" % "1"
"%o" % "1"
"%(key)d" % {"key": "1"}
"%x" % 1.1
"%(key)x" % {"key": 1.1}
"%d" % []
"%d" % ([],)
"%(key)d" % {"key": []}
print("%d" % ("%s" % ("nested",),))
"%d" % ((1, 2, 3),)
"%d" % (1 if x > 0 else [])

# False negatives
WORD = "abc"
"%d" % WORD
"%d %s" % (WORD, WORD)
VALUES_TO_FORMAT = (1, "2", 3.0)
"%d %d %f" % VALUES_TO_FORMAT

# OK
"%d %s %f" % VALUES_TO_FORMAT
"%s" % "1"
"%s %s %s" % ("1", 2, 3.5)
print("%d %d"
      %
(1, 1.1))
"%s" % 1
"%d" % 1
"%f" % 1
"%s" % 1
"%(key)s" % {"key": 1}
"%d" % 1
"%(key)d" % {"key": 1}
"%f" % 1
"%(key)f" % {"key": 1}
"%d" % 1.1
"%(key)d" % {"key": 1.1}
"%s" % []
"%(key)s" % {"key": []}
"%s" % None
"%(key)s" % {"key": None}
print("%s" % ("%s" % ("nested",),))
print("%s" % ("%d" % (5,),))
"%d %d" % "1"
"%d" "%d" % "1"
"-%f" % time.time()
"%r" % (object['dn'],)
r'\%03o' % (ord(c),)
('%02X' % int(_) for _ in o)
"%s;range=%d-*" % (attr, upper + 1)
"%d" % (len(foo),)
'(%r, %r, %r, %r)' % (hostname, address, username, '$PASSWORD')
'%r' % ({'server_school_roles': server_school_roles, 'is_school_multiserver_domain': is_school_multiserver_domain}, )
"%d" % (1 if x > 0 else 2)

# Special cases for %c allowing single character strings
# https://github.com/astral-sh/ruff/issues/8406
"%c" % ("x",)
"%c" % "x"
"%c" % "œ"
