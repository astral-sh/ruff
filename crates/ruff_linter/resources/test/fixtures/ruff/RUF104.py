### No leading space

#noqa
#noqa: PTH123
#ruff: noqa
#ruff: noqa: PTH123
#flake8: noqa
#flake8: noqa: PTH123
#ruff: isort: skip_file


### Too many leading spaces

#  noqa
#   noqa: PTH123
# 	ruff: noqa
#  	ruff: noqa: PTH123
# 	 flake8: noqa
# 	 flake8: noqa: PTH123
#  	    	   ruff: isort: skip_file


### Space before colon

# noqa : PTH123
# ruff : noqa
# ruff : noqa: PTH123
# ruff: noqa : PTH123
# ruff : noqa : PTH123
# flake8 : noqa
# flake8 : noqa: PTH123
# flake8: noqa : PTH123
# flake8 : noqa : PTH123
# ruff : isort: skip_file
# ruff: isort : skip_file
# ruff : isort : skip_file


### No space after colon

# noqa:PTH123
# ruff:noqa
# ruff:noqa: PTH123
# ruff: noqa:PTH123
# ruff:noqa:PTH123
# flake8:noqa
# flake8:noqa: PTH123
# flake8: noqa:PTH123
# flake8:noqa:PTH123
# ruff:isort: skip_file
# ruff: isort:skip_file
# ruff:isort:skip_file


### Too many spaces after colon

# noqa:  		PTH123
# ruff:	 noqa
# ruff:			 	noqa:    	PTH123
# ruff:		 	 noqa:			PTH123
# ruff:   noqa:  PTH123
# flake8:	 			noqa
# flake8: 	noqa:	 		 PTH123
# flake8: 	noqa: 	  PTH123
# flake8: 	 	 noqa:   PTH123
# ruff: 	isort: 			 skip_file
# ruff:  isort: 			skip_file
# ruff:	   isort:		 	skip_file


### Rule list separators

# noqa: PTH123,B012
# noqa: PTH123 ,B012
# noqa: PTH123 , B012
# noqa: PTH123 B012
# noqa: PTH123   B012
# noqa: PTH123 	 B012
# noqa: PTH123,,B012
# noqa: PTH123 ,, B012
# noqa: PTH123 ,	, B012


### Common

#fmt : off
#  fmt:on
# fmt  :skip

#isort: on
#  isort:	off
#	isort:skip
# isort: 	skip_file
#	 isort	:dont-add-imports

# mypy  	:strict
# mypy: disallow-subclassing-any

# 				nopycln		 : import

#pyright:basic
#pyright:strict
# pyright: 	 ignore[reportMissingModuleSource]

#type:ignore
#type :int


### Mix'n'match

#noqa: D101    undocumented-public-class

   # noqa :RUF001,RUF002   # type:ignore
