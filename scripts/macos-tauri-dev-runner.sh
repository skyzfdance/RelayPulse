#!/bin/sh
set -eu

if [ "$#" -lt 1 ]; then
  echo "macOS Tauri development runner requires the compiled executable path." >&2
  exit 2
fi

source_binary=$1
shift

script_directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_directory=$(CDPATH= cd -- "$script_directory/.." && pwd)
app_bundle="$project_directory/src-tauri/target/debug/bundle/macos/RelayPulse.app"
contents_directory="$app_bundle/Contents"
executable_directory="$contents_directory/MacOS"
resources_directory="$contents_directory/Resources"
bundled_binary="$executable_directory/relaypulse"

mkdir -p "$executable_directory" "$resources_directory"
cp "$source_binary" "$bundled_binary"
cp "$script_directory/macos-dev-Info.plist" "$contents_directory/Info.plist"
cp "$project_directory/src-tauri/icons/icon.icns" "$resources_directory/icon.icns"

if [ -d "$project_directory/src-tauri/infoplist" ]; then
  cp -R "$project_directory/src-tauri/infoplist/." "$resources_directory/"
fi

codesign --force --sign - --identifier com.relaypulse.app "$bundled_binary"
codesign --force --deep --sign - --identifier com.relaypulse.app "$app_bundle"

exec "$bundled_binary" "$@"
