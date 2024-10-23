# This file contains test cases only for cases where the logic tests for whether
# the target version is 3.12 or later. A user can have 3.12 syntax even if the target
# version isn't set.

# Quotes reuse
f"{'a'}"

# 312+, it's okay to change the outer quotes even when there's a debug expression using the same quotes
f'foo {10 + len("bar")=}'
f'''foo {10 + len("""bar""")=}'''

# 312+, it's okay to change the quotes here without creating an invalid f-string
f'{"""other " """}'
f'{"""other " """ + "more"}'
f'{b"""other " """}'
f'{f"""other " """}'

