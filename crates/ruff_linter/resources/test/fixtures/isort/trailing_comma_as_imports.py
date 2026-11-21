# A single aliased member is not split, so the trailing comma is respected.
from glob import (
    glob as g,
)

# A statement without aliases is not split, so the trailing comma is respected.
from sys import (
    argv,
    stderr,
)

# Split into one statement per aliased member, so the trailing comma is ignored.
from aws_cdk import (
    NestedStack,
    aws_ecr as ecr,
    aws_ecs as ecs,
    aws_events as events,
    aws_events_targets as targets,
    aws_iam as iam,
    aws_logs as logs,
)
