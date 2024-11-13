#foo
# foo
#ruff: foo-bar
# flake8:foo-bar
#  black:skip
# fmt: foo
# isort: skip_entire
# ruff: isort: skipfile
# yapf: off
# FMT:OFF
# isort: On
# Type: ignore
# yapf: Disable
# Yapf: disable
#yapf : enable
#  yapf : disable

# noqa
# noqa: A123
# noqa: A123, B456
# ruff: noqa
# ruff: noqa: A123
# ruff: noqa: A123, B456
# flake8: noqa
# flake8: noqa: A123
# flake8: noqa: A123, B456
# fmt: on
# fmt: off
# fmt: skip
# isort: on
# isort: off
# isort: split
# isort: skip
# isort: skip_file
# ruff: isort: on
# ruff: isort: skip_file
# type: ignore
# type: int
# type: list[str]
# yapf: enable
# yapf: disable
# noqa:A123
#noqa:   A123
#    type:ignore
#type:	int
# fmt:off
#fmt: on
# 	 fmt:	 skip
# isort:skip
# isort:skip_file
# ruff: isort:skip
# ruff: isort:skip_file
#    type:			ignore
#	 	 	type:		 	int
#	  	yapf: 	 	enable
#		yapf: 		disable

# NoQA: A123, B456
# ruff: NoQA: A123, B456
# flake8: NoQA: A123, B456

# noqa: A123 B456
# ruff: noqa: A123 B456
# flake8: noqa: A123 B456
# noqa: A123,B456
# ruff: noqa: A123,B456
# flake8: noqa: A123,B456
# noqa: A123,,B456
# noqa: A123 , 	,	 	B456
# noqa: A123	B456
# noqa: A123			B456
# noqa: A123				B456
# noqa: A123 ,B456
# ruff: noqa: A123	B456
# flake8: noqa: A123   B456


# type: ignore  # noqa: A123, B456

#isort:skip#noqa:A123

# fmt:off#   noqa: A123
# noqa:A123, B456 - Lorem ipsum dolor sit amet
