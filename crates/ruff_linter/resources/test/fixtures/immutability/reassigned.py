"""RUF066 - Single-assignment missing Final.

Should NOT warn - Variable is reassigned, so RUF066 does not apply.
"""

X = 1
X = 2
print(X)
