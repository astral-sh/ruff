"""
Test cases from lambda.py that cause an instability in the stable
implementation but that are handled by the preview `indent_lambda_parameters`
version.
"""

(
    lambda
    # comment 1
    *
    # comment 2
    x,
    **y:
    # comment 3
    x
)

(
    lambda # comment 1
        * # comment 2
        x,
        y: # comment 3
    x
)
