"""Test case for detecting @apply_defaults decorator usage in AIR301."""
from airflow.models.baseoperator import BaseOperator
from airflow.utils.decorators import apply_defaults


class DecoratedOperator(BaseOperator):
    @apply_defaults
    def __init__(self, message, **kwargs):
        super().__init__(**kwargs)
        self.message = message

    def execute(self, context):
        print(self.message)

class ExampleOperator(BaseOperator):
    def __init__(self, message, **kwargs):
        super().__init__(**kwargs)
        self.message = message

    def execute(self, context):
        print(self.message)
