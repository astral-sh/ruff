# Nested string literals inside t-string expressions follow either alternating
# or preferred quote normalization depending on nested-string-quote-style.
t'{ "nested" }'
t'{ "nested" = }'
t'{ ["1", "2"] }'
