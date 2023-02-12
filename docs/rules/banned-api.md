# banned-api (TID251)

Derived from the **flake8-tidy-imports** linter.

## What it does
Checks for banned imports.

## Why is this bad?
Projects may want to ensure that specific modules or module members are
not be imported or accessed.

Security or other company policies may be a reason to impose
restrictions on importing external Python libraries. In some cases,
projects may adopt conventions around the use of certain modules or
module members that are not enforceable by the language itself.

This rule enforces certain import conventions project-wide in an
automatic way.

## Options
* `flake8-tidy-imports.banned-api`