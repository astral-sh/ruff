set -euxo pipefail

NAME=$1

mkdir -p src/$1
mkdir -p resources/test/fixtures/$1
touch src/$1/mod.rs

sed -i "" "s/mod flake8_print;/mod flake8_print; mod flake8_return;/g" src/lib.rs
sed -i "" "s|// flake8-print|// flake8-return\n// flake8-print|g" src/checks.rs

