## Migration

This diagnostic code is deprecated. ty now reports name mismatches under a separate rule for each
kind of type:

- `mismatched-enum-name`
- `mismatched-namedtuple-name`
- `mismatched-newtype-name`
- `mismatched-paramspec-name`
- `mismatched-type-alias-name`
- `mismatched-typeddict-name`
- `mismatched-typevar-name`
- `mismatched-typevartuple-name`

Existing configuration entries and ignore comments using `mismatched-type-name` still apply to all
eight rules. However, users are encouraged to migrate to using the new rules in configuration files
and suppression comments wherever possible.
