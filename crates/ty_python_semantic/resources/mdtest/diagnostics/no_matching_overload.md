# No matching overload diagnostics

<!-- snapshot-diagnostics -->

## Calls to overloaded functions

TODO: Note that we do not yet support the `@overload` decorator to define overloaded functions in
real Python code. We are instead testing a special-cased function where we create an overloaded
signature internally. Update this to an `@overload` function in the Python snippet itself once we
can.

```py
type("Foo", ())  # error: [no-matching-overload]
```
