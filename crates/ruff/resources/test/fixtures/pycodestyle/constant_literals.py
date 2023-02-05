###
# Errors
###
if "abc" is "def":  # F632 (fix)
    pass
if "abc" is None:  # F632 (fix, but leaves behind unfixable E711)
    pass
if None is "abc":  # F632 (fix, but leaves behind unfixable E711)
    pass
if "abc" is False:  # F632 (fix, but leaves behind unfixable E712)
    pass
if False is "abc":  # F632 (fix, but leaves behind unfixable E712)
    pass
if False == None:  # E711, E712 (fix)
    pass
if None == False:  # E711, E712 (fix)
    pass

###
# Unfixable errors
###
if "abc" == None:  # E711
    pass
if None == "abc":  # E711
    pass
if "abc" == False:  # E712
    pass
if False == "abc":  # E712
    pass

###
# Non-errors
###
if "def" == "abc":
    pass
if False is None:
    pass
if None is False:
    pass
