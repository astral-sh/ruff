###
# Positive cases
###

a_dict = {}

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

# OK (default contains effect)
if key in a_dict:
    var = a_dict[key]
else:
    var = val1 + val2

# SIM401 (complex expression in key)
if keys[idx] in a_dict:
    var = a_dict[keys[idx]]
else:
    var = "default"

dicts = {"key": a_dict}

# SIM401 (complex expression in dict)
if key in dicts[idx]:
    var = dicts[idx][key]
else:
    var = "default"

# SIM401 (complex expression in var)
if key in a_dict:
    vars[idx] = a_dict[key]
else:
    vars[idx] = "defaultß9💣2ℝ6789ß9💣2ℝ6789ß9💣2ℝ6789ß9💣2ℝ6789ß9💣2ℝ6789"

# SIM401
if foo():
    pass
else:
    if key in a_dict:
        vars[idx] = a_dict[key]
    else:
        vars[idx] = "default"

###
# Negative cases
###

# OK (too long)
if key in a_dict:
    vars[idx] = a_dict[key]
else:
    vars[idx] = "defaultß9💣2ℝ6789ß9💣2ℝ6789ß9💣2ℝ6789ß9💣2ℝ6789ß9💣2ℝ6789ß"

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

class NotADictionary:
    def __init__(self):
        self._dict = {}

    def __getitem__(self, key):
        return self._dict[key]

    def __setitem__(self, key, value):
        self._dict[key] = value

    def __iter__(self):
        return self._dict.__iter__()

not_dict = NotADictionary()
not_dict["key"] = "value"

# OK (type `NotADictionary` is not a known dictionary type)
if "key" in not_dict:
    value = not_dict["key"]
else:
    value = None

###
# Positive cases (if expressions)
###

# SIM401
var = a_dict[key] if key in a_dict else "default3"

# SIM401
var = "default-1" if key not in a_dict else a_dict[key]

# OK (default contains effect)
var = a_dict[key] if key in a_dict else val1 + val2

###
# Dict subclasses overriding `get`
###

class Cache(dict):
    def get(self, key, default=None):
        # OK (`self` is a dict subclass, not a known plain dict; rewriting
        # this to `self.get(...)` would call this method again).
        return self[key] if key in self else default


cache = Cache()

# OK (a dict subclass may override `get`)
var = cache[key] if key in cache else "default4"

###
# Lambda parameter defaults
###

# SIM401: literal defaults have no side effects.
if key in a_dict:
    var = a_dict[key]
else:
    var = lambda x=0, /, y=1, *, z=2: (x, y, z)

# OK: dict.get would evaluate the lambda's default even when the key exists.
if key in a_dict:
    var = a_dict[key]
else:
    var = lambda value=initialize(): value

# OK: positional-only defaults are also evaluated when the lambda is created.
if key in a_dict:
    var = a_dict[key]
else:
    var = lambda value=initialize(), /: value

# OK: keyword-only defaults can contain nested side effects.
if key in a_dict:
    var = a_dict[key]
else:
    var = lambda *, value=(initialize(),): value
