# Red Knot

Red Knot is an extremely fast type checker.
Currently, it is a work-in-progress and not ready for user testing.

Red Knot is designed to prioritize good type inference, even in unannotated code,
and aims to avoid false positives.

While Red Knot will produce similar results to mypy and pyright on many codebases,
100% compatibility with these tools is a non-goal.
On some codebases, Red Knot's design decisions lead to different outcomes
than you would get from running one of these more established tools.

## Contributing

Core type checking tests are written as Markdown code blocks.
They can be found in [`red_knot_python_semantic/resources/mdtest`][resources-mdtest].
See [`red_knot_test/README.md`][mdtest-readme] for more information
on the test framework itself.

The list of open issues can be found [here][open-issues].

[mdtest-readme]: ../red_knot_test/README.md
[open-issues]: https://github.com/astral-sh/ruff/issues?q=sort%3Aupdated-desc%20is%3Aissue%20is%3Aopen%20label%3Ared-knot
[resources-mdtest]: ../red_knot_python_semantic/resources/mdtest
