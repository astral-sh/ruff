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

named_var = []
if [] is []:  # F632 (fix)
    pass
if named_var is []:  # F632 (fix)
    pass
if [] is named_var:  # F632 (fix)
    pass
if named_var is [1]:  # F632 (fix)
    pass
if [1] is named_var:  # F632 (fix)
    pass
if named_var is [i for i in [1]]:  # F632 (fix)
    pass

named_var = {}
if {} is {}:  # F632 (fix)
    pass
if named_var is {}:  # F632 (fix)
    pass
if {} is named_var:  # F632 (fix)
    pass
if named_var is {1}:  # F632 (fix)
    pass
if {1} is named_var:  # F632 (fix)
    pass
if named_var is {i for i in [1]}:  # F632 (fix)
    pass

named_var = {1: 1}
if {1: 1} is {1: 1}:  # F632 (fix)
    pass
if named_var is {1: 1}:  # F632 (fix)
    pass
if {1: 1} is named_var:  # F632 (fix)
    pass
if named_var is {1: 1}:  # F632 (fix)
    pass
if {1: 1} is named_var:  # F632 (fix)
    pass
if named_var is {i: 1 for i in [1]}:  # F632 (fix)
    pass

###
# Non-errors
###
if "abc" == None:
    pass
if None == "abc":
    pass
if "abc" == False:
    pass
if False == "abc":
    pass
if "def" == "abc":
    pass
if False is None:
    pass
if None is False:
    pass

named_var = []
if [] == []:
    pass
if named_var == []:
    pass
if [] == named_var:
    pass
if named_var == [1]:
    pass
if [1] == named_var:
    pass
if named_var == [i for i in [1]]:
    pass

named_var = {}
if {} == {}:
    pass
if named_var == {}:
    pass
if {} == named_var:
    pass
if named_var == {1}:
    pass
if {1} == named_var:
    pass
if named_var == {i for i in [1]}:
    pass

named_var = {1: 1}
if {1: 1} == {1: 1}:
    pass
if named_var == {1: 1}:
    pass
if {1: 1} == named_var:
    pass
if named_var == {1: 1}:
    pass
if {1: 1} == named_var:
    pass
if named_var == {i: 1 for i in [1]}:
    pass
