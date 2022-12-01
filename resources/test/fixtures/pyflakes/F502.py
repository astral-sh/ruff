dog = {"bob": "bob"}

"%(bob)s" % dog
"%(bob)s" % {"bob": "bob"}
"%(bob)s" % {**{"bob": "bob"}}
"%(bob)s" % ["bob"]  # F202
"%(bob)s" % ("bob",)  # F202
"%(bob)s" % {"bob"}  # F202
"%(bob)s" % [*["bob"]]  # F202
"%(bob)s" % {"bob": "bob" for _ in range(1)}
"%(bob)s" % ["bob" for _ in range(1)]  # F202
"%(bob)s" % ("bob" for _ in range(1))  # F202
"%(bob)s" % {"bob" for _ in range(1)}  # F202
