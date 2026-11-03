## Migration

ty now reports name mismatches under a separate rule for each kind of type:

- `mismatched-enum-name`
- `mismatched-namedtuple-name`
- `mismatched-newtype-name`
- `mismatched-paramspec-name`
- `mismatched-type-alias-name`
- `mismatched-typeddict-name`
- `mismatched-typevar-name`
- `mismatched-typevartuple-name`

Existing configuration entries and ignore comments using `mismatched-type-name` still apply to all
eight rules. A specific rule takes precedence within the same table. For example, to keep a policy
that treats name mismatches as errors but allow named tuples to have a different runtime name, use
these settings in `ty.toml`:

```toml
[rules]
mismatched-type-name = "error"
mismatched-namedtuple-name = "ignore"
```

On the command line, the last matching flag takes precedence.
`ty check --error mismatched-type-name --ignore mismatched-namedtuple-name` applies the same policy.

The separate `mismatched-sentinel-name` rule remains opt-in.
