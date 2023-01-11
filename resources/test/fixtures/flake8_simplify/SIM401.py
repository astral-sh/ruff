
if key in a_dict:  # pattern-1
    var = a_dict[key]
else:
    var = "default1"

if key not in a_dict:  # pattern-2
    var = "default2"
else:
    var = a_dict[key]

if key in a_dict:  # default has complex expr
    var = a_dict[key]
else:
    var = val1 + val2


# Negative cases
if key in a_dict:  # different dict
    var = other_dict[key]
else:
    var = "default"

if key in a_dict:  # different key
    var = a_dict[other_key]
else:
    var = "default"

if key in a_dict:  # different var
    var = a_dict[key]
else:
    other_var = "default"

if key in a_dict:  # extra vars in body
    var = a_dict[key]
    var2 = value2
else:
    var = "default"

if key in a_dict:  # extra vars in orelse
    var = a_dict[key]
else:
    var2 = value2
    var = "default"

if not key in a_dict:  # negated test : could be a nice-to-have
    var = "default"
else:
    var = a_dict[key]

if keys[idx] in a_dict:  # complex expr in key : nice-to-have
    var = a_dict[keys[idx]]
else:
    var = "default"

if key in dicts[idx]:  # complex expr in dict : nice-to-have
    var = dicts[idx][key]
else:
    var = "default"

if key in a_dict:   # complex expr in var: nice-to-have
    vars[idx] = a_dict[key]
else:
    vars[idx] = "default"
