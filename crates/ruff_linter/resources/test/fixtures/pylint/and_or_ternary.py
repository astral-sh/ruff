# OK

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

# Don't emit if the parent is an `if` statement.
if (task_id in task_dict and task_dict[task_id] is not task) \
        or task_id in used_group_ids:
    pass

no_target, is_x64, target = True, False, 'target'
if (no_target and not is_x64) or target == 'ARM_APPL_RUST_TARGET':
    pass

# Don't emit if the parent is a `bool_op` expression.
isinstance(val, str) and ((len(val) == 7 and val[0] == "#") or val in enums.NamedColor)

# Errors

1<2 and 'a' or 'b'

(lambda x: x+1) and 'a' or 'b'

'a' and (lambda x: x+1) or 'orange'

val = '#0000FF'
(len(val) == 7 and val[0] == "#") or val in {'green'}

marker = 'marker'
isinstance(marker, dict) and 'field' in marker or marker in {}

def has_oranges(oranges, apples=None) -> bool:
    return apples and False or oranges

[x for x in l if a and b or c]

{x: y for x in l if a and b or c}

{x for x in l if a and b or c}

new_list = [
    x
    for sublist in all_lists
    if a and b or c
    for x in sublist
    if (isinstance(operator, list) and x in operator) or x != operator
]
