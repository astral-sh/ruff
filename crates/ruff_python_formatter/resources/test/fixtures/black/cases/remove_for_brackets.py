# Only remove tuple brackets after `for`
for (k, v) in d.items():
    print(k, v)

# Don't touch tuple brackets after `in`
for module in (core, _unicodefun):
    if hasattr(module, "_verify_python3_env"):
        module._verify_python3_env = lambda: None

# Brackets remain for long for loop lines
for (why_would_anyone_choose_to_name_a_loop_variable_with_a_name_this_long, i_dont_know_but_we_should_still_check_the_behaviour_if_they_do) in d.items():
    print(k, v)

for (k, v) in dfkasdjfldsjflkdsjflkdsjfdslkfjldsjfgkjdshgkljjdsfldgkhsdofudsfudsofajdslkfjdslkfjldisfjdffjsdlkfjdlkjjkdflskadjldkfjsalkfjdasj.items():
    print(k, v)

# Test deeply nested brackets
for (((((k, v))))) in d.items():
    print(k, v)
