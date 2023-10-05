
# No Ternary (No Error)

1<2 and 'b' and 'c'

1<2 or 'a' and 'b'

1<2 and 'a'

1<2 or 'a'

2>1

1<2 and 'a' or 'b' and 'c'

1<2 and 'a' or 'b' or 'c'

1<2 and 'a' or 'b' or 'c' or (lambda x: x+1)

1<2 and 'a' or 'b' or (lambda x: x+1) or 'c'

default = 'default'
if (not isinstance(default, bool) and isinstance(default, int)) \
        or (isinstance(default, str) and default):
    pass

docid, token = None, None
(docid is None and token is None) or (docid is not None and token is not None)

vendor, os_version = 'darwin', '14'
vendor == "debian" and os_version in ["12"] or vendor == "ubuntu" and os_version in []

# Ternary (Error)

1<2 and 'a' or 'b'

(lambda x: x+1) and 'a' or 'b'

'a' and (lambda x: x+1) or 'orange'

val = '#0000FF'
(len(val) == 7 and val[0] == "#") or val in {'green'}

marker = 'marker'
isinstance(marker, dict) and 'field' in marker or marker in {}

def has_oranges(oranges, apples=None) -> bool:
    return apples and False or oranges
