def triggered_if_if_block_ends_with_elif(machine, old_conf, new_conf):
    """Example code that will trigger the message

    Given an if-elif construct
    When the body of the if ends with an elif
    Then the message confusing-consecutive-elif must be triggered.
    """
    if old_conf:
        if not new_conf:
            machine.disable()
        elif old_conf.value != new_conf.value:
            machine.disable()
            machine.enable(new_conf.value)
    elif new_conf:  # [confusing-consecutive-elif]
        machine.enable(new_conf.value)


def not_triggered_if_indented_block_ends_with_else(machine, old_conf, new_conf):
    """Example code must not trigger the message, because the inner block ends with else.

    Given an if-elif construct
    When the body of the if ends with an else
    Then no message shall be triggered.
    """
    if old_conf:
        if not new_conf:
            machine.disable()
        elif old_conf.value != new_conf.value:
            machine.disable()
            machine.enable(new_conf.value)
        else:
            pass
    elif new_conf:
        machine.enable(new_conf.value)


def not_triggered_if_indentend_block_ends_with_call(machine, old_conf, new_conf):
    """
    Example code must not trigger the message,

    Given an if-elif construct
    When the body of the if ends with a function call
    Then no message shall be triggered.

    Note: There is nothing special about the body ending with a function call.
    This is just taken as a representative value for the equivalence class of
    "every node class unrelated to if/elif/else".
    """
    if old_conf:
        if not new_conf:
            machine.disable()
        elif old_conf.value != new_conf.value:
            machine.disable()
            machine.enable(new_conf.value)
        print("Processed old configuration...")
    elif new_conf:
        machine.enable(new_conf.value)


def triggered_if_elif_block_ends_with_elif(machine, old_conf, new_conf, new_new_conf):
    """Example code that will trigger the message

    Given an if-elif-elif construct
    When the body of the first elif ends with an elif
    Then the message confusing-consecutive-elif must be triggered.
    """
    if old_conf:
        machine.disable()
    elif not new_conf:
        if new_new_conf:
            machine.disable()
        elif old_conf.value != new_conf.value:
            machine.disable()
            machine.enable(new_conf.value)
    elif new_conf:  # [confusing-consecutive-elif]
        machine.enable(new_conf.value)


def triggered_if_block_ends_with_if(machine, old_conf, new_conf, new_new_conf):
    """Example code that will trigger the message

    Given an if-elif construct
    When the body of the if ends with an if
    Then the message confusing-consecutive-elif must be triggered.
    """
    if old_conf:
        if new_new_conf:
            machine.disable()
    elif new_conf:  # [confusing-consecutive-elif]
        machine.enable(new_conf.value)


def not_triggered_if_indented_block_ends_with_ifexp(machine, old_conf, new_conf):
    """
    Example code must not trigger the message,

    Given an if-elif construct
    When the body of the if ends with an if expression
    Then no message shall be triggered.
    """
    if old_conf:
        if not new_conf:
            machine.disable()
        print("Processed old configuration...")
    elif new_conf:
        machine.enable(new_conf.value)


def not_triggered_if_outer_block_does_not_have_elif(machine, old_conf, new_conf):
    """Example code must not trigger the message

    Given an if construct without an elif
    When the body of the if ends with an if
    Then no message shall be triggered.
    """
    if old_conf:
        if not new_conf:
            machine.disable()
        elif old_conf.value != new_conf.value:
            machine.disable()
            machine.enable(new_conf.value)
    else:
        pass


def not_triggered_if_outer_block_continues_with_if(machine, old_conf, new_conf, new_new_conf):
    """Example code that will trigger the message

    Given an if construct which continues with a new if construct
    When the body of the first if ends with an if expression
    Then no message shall be triggered.
    """
    if old_conf:
        if new_new_conf:
            machine.disable()
        elif old_conf.value != new_conf.value:
            machine.disable()
            machine.enable(new_conf.value)
    if new_conf:
        machine.enable(new_conf.value)
