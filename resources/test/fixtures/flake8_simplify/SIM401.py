###
# Positive cases
###

# SIM401 (pattern-1)
if key in a_dict:
    var = a_dict[key]
else:
    var = "default1"

# SIM401 (pattern-2)
if key not in a_dict:
    var = "default2"
else:
    var = a_dict[key]

# SIM401 (default with a complex expression)
if key in a_dict:
    var = a_dict[key]
else:
    var = val1 + val2

# SIM401 (complex expression in key)
if keys[idx] in a_dict:
    var = a_dict[keys[idx]]
else:
    var = "default"

# SIM401 (complex expression in dict)
if key in dicts[idx]:
    var = dicts[idx][key]
else:
    var = "default"

# SIM401 (complex expression in var)
if key in a_dict:
    vars[idx] = a_dict[key]
else:
    vars[idx] = "default"

###
# Negative cases
###

# OK (false negative)
if not key in a_dict:
    var = "default"
else:
    var = a_dict[key]

# OK (different dict)
if key in a_dict:
    var = other_dict[key]
else:
    var = "default"

# OK (different key)
if key in a_dict:
    var = a_dict[other_key]
else:
    var = "default"

# OK (different var)
if key in a_dict:
    var = a_dict[key]
else:
    other_var = "default"

# OK (extra vars in body)
if key in a_dict:
    var = a_dict[key]
    var2 = value2
else:
    var = "default"

# OK (extra vars in orelse)
if key in a_dict:
    var = a_dict[key]
else:
    var2 = value2
    var = "default"

# OK (complex default value)
if key in a_dict:
    var = a_dict[key]
else:
    var = foo()

# OK (complex default value)
if key in a_dict:
    var = a_dict[key]
else:
    var = a_dict["fallback"]

# OK (false negative for elif)
if foo():
    pass
elif key in a_dict:
    vars[idx] = a_dict[key]
else:
    vars[idx] = "default"

# OK (false negative for nested else)
if foo():
    pass
else:
    if key in a_dict:
        vars[idx] = a_dict[key]
    else:
        vars[idx] = "default"
