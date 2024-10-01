def correct_fruits(fruits) -> bool:
    if len(fruits) > 1:  # PLR1702
        if "apple" in fruits:
            if "orange" in fruits:
                count = fruits["orange"]
                if count % 2:
                    if "kiwi" in fruits:
                        if count == 2:
                            return True
    return False

# Ok
def correct_fruits(fruits) -> bool:
    if len(fruits) > 1:
        if "apple" in fruits:
            if "orange" in fruits:
                count = fruits["orange"]
                if count % 2:
                    if "kiwi" in fruits:
                        return True
    return False
