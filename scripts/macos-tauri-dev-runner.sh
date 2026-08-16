#!/bin/sh
set -eu

if [ "$#" -lt 1 ]; then
  echo "macOS Tauri development runner requires the compiled executable path." >&2
  exit 2
fi

source_binary=$1
shift

if [ ! -f "$source_binary" ] || [ ! -r "$source_binary" ] || [ ! -x "$source_binary" ]; then
  echo "macOS Tauri development runner requires a readable executable file: $source_binary" >&2
  exit 2
fi

script_directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_directory=$(CDPATH= cd -- "$script_directory/.." && pwd)
runner_directory="$project_directory/src-tauri/target/debug/macos-dev-runners"
mkdir -p "$runner_directory"

invocation_directory=$(mktemp -d "$runner_directory/relaypulse.XXXXXX")

cleanup() {
  status=$?
  trap - EXIT
  case "$invocation_directory" in
    "$runner_directory"/relaypulse.*) rm -rf "$invocation_directory" ;;
    *) echo "Refusing to remove unexpected runner directory: $invocation_directory" >&2 ;;
  esac
  exit "$status"
}

trap cleanup EXIT

app_bundle="$invocation_directory/RelayPulse.app"
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

"$bundled_binary" "$@"
