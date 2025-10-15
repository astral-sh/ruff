d = {}
l = []


### Errors

if k in d:                          # Bare name
    del d[k]

if '' in d:                         # String
    del d[""]                       # Different quotes

if b"" in d:                        # Bytes
    del d[                          # Multiline slice
        b''''''                     # Triple quotes
    ]

if 0 in d: del d[0]                 # Single-line statement

if 3j in d:                         # Complex
    del d[3j]

if 0.1234 in d:                     # Float
    del d[.1_2_3_4]                 # Number separators and shorthand syntax

if True in d:                       # True
    del d[True]

if False in d:                      # False
    del d[False]

if None in d:                       # None
    del d[
        # Comment in the middle
        None
    ]

if ... in d:                        # Ellipsis
    del d[
                                    # Comment in the middle, indented
        ...]

if "a" "bc" in d:                   # String concatenation
    del d['abc']

if r"\foo" in d:                    # Raw string
    del d['\\foo']

if b'yt' b'es' in d:                # Bytes concatenation
    del d[rb"""ytes"""]             # Raw bytes

if k in d:
    # comment that gets dropped
    del d[k]

### Safely fixable

if k in d:
    del d[k]

if '' in d:
    del d[""]

if b"" in d:
    del d[
        b''''''
    ]

if 0 in d: del d[0]

if 3j in d:
    del d[3j]

if 0.1234 in d:
    del d[.1_2_3_4]

if True in d:
    del d[True]

if False in d:
    del d[False]

if None in d:
    del d[
        None
    ]

if ... in d:
    del d[
        ...]

if "a" "bc" in d:
    del d['abc']

if r"\foo" in d:
    del d['\\foo']

if b'yt' b'es' in d:
    del d[rb"""ytes"""]             # This should not make the fix unsafe



### No errors

if k in l:                          # Not a dict
    del l[k]

if d.__contains__(k):               # Explicit dunder call
    del d[k]

if a.k in d:                        # Attribute
    del d[a.k]

if (a, b) in d:                     # Tuple
    del d[a, b]

if 2 in d:                          # Different key value (int)
    del d[3]

if 2_4j in d:                       # Different key value (complex)
    del d[3.6]                      # Different key value (float)

if 0.1 + 0.2 in d:                  # Complex expression
    del d[0.3]

if f"0" in d:                       # f-string
    del d[f"0"]

if k in a.d:                        # Attribute dict
    del a.d[k]

if k in d:                          # else statement
    del d[k]
else:
    pass

if k in d:                          # elif and else statements
    del d[k]
elif 0 in d:
    del d[0]
else:
    pass
