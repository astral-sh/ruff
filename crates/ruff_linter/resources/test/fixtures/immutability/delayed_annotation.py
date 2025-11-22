"""RUF066 - Single-assignment missing Final.

Should NOT warn — The variable gets an annotation later.
"""

X = 1
X: int
print(X)
