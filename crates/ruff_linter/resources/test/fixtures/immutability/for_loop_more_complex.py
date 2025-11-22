"""RUF066 - Single-assignment missing Final.

Should NOT warn - Even though the variable
would never be reassigned, RUF066 currently
does not handle more complex cases like the
one below.
"""

FLAG = 0
for i in range(4):
    if i == 5:
        FLAG = 1
print(FLAG)
