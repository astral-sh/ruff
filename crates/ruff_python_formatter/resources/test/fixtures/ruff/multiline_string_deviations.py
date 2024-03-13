# This file documents the deviations for formatting multiline strings with black.

# Black hugs the parentheses for `%` usages -> convert to fstring.
# Can get unreadable if the arguments split
# This could be solved by using `best_fitting` to try to format the arguments on a single
# line. Let's consider adding this later.
# ```python
# call(
#    3,
#    "dogsay",
#    textwrap.dedent(
#        """dove
#    coo""" % "cowabunga",
#        more,
#        and_more,
#        "aaaaaaa",
#        "bbbbbbbbb",
#        "cccccccc",
#    ),
# )
# ```
call(3, "dogsay", textwrap.dedent("""dove
    coo""" % "cowabunga"))

# Black applies the hugging recursively. We don't (consistent with the hugging style).
path.write_text(textwrap.dedent("""\
    A triple-quoted string
    actually leveraging the textwrap.dedent functionality
    that ends in a trailing newline,
    representing e.g. file contents.
"""))



# Black avoids parenthesizing the following lambda. We could potentially support
# this by changing `Lambda::needs_parentheses` to return `BestFit` but it causes
# issues when the lambda has comments.
# Let's keep this as a known deviation for now.
generated_readme = lambda project_name: """
{}

<Add content here!>
""".strip().format(project_name)
