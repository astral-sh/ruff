#!/usr/bin/env nix-shell
#!nix-shell -I nixpkgs=./. -i python3 -p "requests" -p ruff
print("Hello world")
