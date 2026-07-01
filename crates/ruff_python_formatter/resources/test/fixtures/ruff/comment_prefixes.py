# Comments are normalized by inserting a single leading space after the `#`,
# unless they start with one of a few special characters. Those are preserved
# verbatim so that tooling relying on a specific prefix keeps working.

#! shebang-style comments are left as-is
#: Sphinx-style comments are left as-is
#' pweave-style comments are left as-is
#| Quarto cell options are left as-is
