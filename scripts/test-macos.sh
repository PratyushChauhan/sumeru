#!/usr/bin/env bash
# Inputs: none. Outputs: `swift test` for macos/ (adds the Command Line Tools' Testing.framework
# search paths when Xcode is not installed; with Xcode, plain `swift test` already works).
set -euo pipefail
cd "$(dirname "$0")/../macos"
F=/Library/Developer/CommandLineTools/Library/Developer/Frameworks
if ! xcodebuild -version >/dev/null 2>&1 && [ -d "$F" ]; then
  L=/Library/Developer/CommandLineTools/Library/Developer/usr/lib
  exec swift test -Xswiftc "-F$F" -Xlinker "-F$F" -Xlinker -rpath -Xlinker "$F" -Xlinker -rpath -Xlinker "$L"
fi
exec swift test
