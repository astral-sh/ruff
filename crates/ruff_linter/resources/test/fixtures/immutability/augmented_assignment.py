"""RUF066 - Single-assignment missing Final.

Should NOT warn — X is reassigned (X += 2).
"""

X = 1
X += 2
print(X)
