# These SHOULD change
"%(k)s" % {"k": "v"}

"%(k)s" % {
    "k": "v",
    "i": "j"
}

"%(to_list)s" % {"to_list": []}

"%(k)s" % {"k": "v", "i": 1, "j": []}

paren_continue = (
    "foo %(foo)s "
    "bar %(bar)s" % {"foo": x, "bar": y}
)

paren_string = (
    "foo %(foo)s "
    "bar %(bar)s"
) % {"foo": x, "bar": y}

""" Waiting for Charlie to review my regex before uncommenting this
"%(foo)s \N{snowman}" % {"foo": 1}
"""

# Make sure to test assignement, call. and multi-line
# These should NOT change
"%()s" % {"": "empty"}

"%s" % {"k": "v"}

"%(1)s" % {"1": "bar"}

"%(a)s" % {"a": 1, "a": 2}

# Asking Charlie if we can ignore this one
# "%(ab)s" % {"a" "b": 1}

# Asking Charlie if we can ignore this one
# "%(a)s" % {"a"  :  1}

"%(1)s" % {1: 2, "1": 2}

"%(and)s" % {"and": 2}

"%" % {}

"%()s" % {"": "bar"}
