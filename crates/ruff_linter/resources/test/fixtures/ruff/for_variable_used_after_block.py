for global_var in []:
    _ = global_var
    pass

# ❌
_ = global_var

def foo():
    # For control var used outside block
    for event in []:
        _ = event
        pass

    # ❌
    _ = event

    for x in []:
        _ = x
        pass
    
    # Using the same control variable name in a different loop is ok
    for x in []:
        _ = x
        pass

    for y in []:
        # ❌ x is used outside of the loop it was defined in (meant to use y)
        if x == 5:
            pass

    # Assign a variable before the loop
    room_id = 3
    _ = room_id
    # Use the same variable name in a loop
    for room_id in []:
        _ = room_id
        pass

    # ❌ After the loop is not ok because the value is probably not what you expect
    _ = room_id

    # ❌ Augmented assignment is not allowed because the value is probably not what you expect
    room_id += 1

    # Assigning again after the loop is ok
    room_id = 5
    room_id += 1
    _ = room_id

    # Tuple destructuring
    for a, b, c in []:
        _ = a
        _ = b
        _ = c
        pass

    # ❌
    _ = a
    _ = b
    _ = c

    # Array destructuring
    for [d, e, f] in []:
        _ = d
        _ = e
        _ = f
        pass

    # ❌
    _ = d
    _ = e
    _ = f

    # Nested function and class definitions are fine
    for potential_power in []:
        def action():
            print(potential_power)

        class Animal:
            power_level = potential_power

