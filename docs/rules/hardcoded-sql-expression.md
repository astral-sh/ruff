# hardcoded-sql-expression (S608)

Derived from the **flake8-bandit** linter.

### What it does
Checks for strings that resemble SQL statements involved in some form
string building operation.

### Why is this bad?
SQL injection is a common attack vector for web applications. Unless care
is taken to sanitize and control the input data when building such
SQL statement strings, an injection attack becomes possible.

### Example
```python
query = "DELETE FROM foo WHERE id = '%s'" % identifier
```