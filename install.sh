#!/bin/sh
# aski — installer shim
#
# Delegates to the cargo-dist-generated installer for the latest release.
# This exists so the install and uninstall one-liners share a URL shape:
#
#     curl --proto '=https' --tlsv1.2 -LsSf https://raw.githubusercontent.com/excelano/aski/main/install.sh | sh
#     curl --proto '=https' --tlsv1.2 -LsSf https://raw.githubusercontent.com/excelano/aski/main/uninstall.sh | sh

set -eu

curl --proto '=https' --tlsv1.2 -LsSf \
    https://github.com/excelano/aski/releases/latest/download/aski-installer.sh | sh
