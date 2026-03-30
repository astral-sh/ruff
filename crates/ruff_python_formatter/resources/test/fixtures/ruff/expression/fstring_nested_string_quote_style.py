# Nested string literals inside f-string expressions follow either alternating
# or preferred quote normalization depending on nested-string-quote-style.
f'{ "nested" }'
f'{ "nested" = }'
f'{ ["1", "2"] }'
