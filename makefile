.DEFAULT_GOAL := help

build: # Build ruff tool image
	@docker compose build

TARGET = $(word 2,$(MAKECMDGOALS))
SOURCE = $(word 3,$(MAKECMDGOALS))
ifeq ($(SOURCE), )
	SOURCE = $(PWD)
endif

check: # Run Ruff on the given files or directories
	@docker run -itv $(SOURCE):/target --rm ruff check /target/$(TARGET)

format: # Run the Ruff formatter on the given files or directories
	@docker run -itv $(SOURCE):/target --rm ruff format /target/$(TARGET)

help:
	@egrep -h '\s#\s' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*# "}; {printf "\033[36m%-30s\033[0m %s.\n", $$1, $$2}'

%: ;
