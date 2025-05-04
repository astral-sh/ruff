# ty

ty is an extremely fast type checker.
Currently, it is a work-in-progress and not ready for user testing.

ty is designed to prioritize good type inference, even in unannotated code,
and aims to avoid false positives.

While ty will produce similar results to mypy and pyright on many codebases,
100% compatibility with these tools is a non-goal.
On some codebases, ty's design decisions lead to different outcomes
than you would get from running one of these more established tools.

## Contributing

Core type checking tests are written as Markdown code blocks.
They can be found in [`ty_python_semantic/resources/mdtest`][resources-mdtest].
See [`ty_test/README.md`][mdtest-readme] for more information
on the test framework itself.

The list of open issues can be found [here][open-issues].

[mdtest-readme]: ../ty_test/README.md
[open-issues]: https://github.com/astral-sh/ty/issues
[resources-mdtest]: ../ty_python_semantic/resources/mdtest
