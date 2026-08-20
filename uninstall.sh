#!/bin/sh
# aski — uninstaller
#
# Removes the aski binary installed by install.sh. The config file is left where
# it is: it is the part you wrote, an uninstall is often a reinstall, and nothing
# else on disk belongs to aski (no history, no cache).
#
# To remove that too:
#     rm -rf "${XDG_CONFIG_HOME:-$HOME/.config}/aski"
#
#     curl --proto '=https' --tlsv1.2 -LsSf https://raw.githubusercontent.com/excelano/aski/main/uninstall.sh | sh

set -eu

if [ -n "${CARGO_HOME:-}" ]; then
    install_dir="$CARGO_HOME/bin"
else
    install_dir="$HOME/.cargo/bin"
fi

target="$install_dir/aski"

if [ -e "$target" ]; then
    rm -f "$target"
    echo "Removed $target"
elif command -v aski >/dev/null 2>&1; then
    found="$(command -v aski)"
    echo "aski is installed at $found, not the expected location ($target)."
    echo "Remove it manually if you want it gone."
    exit 1
else
    echo "aski is not installed."
fi
