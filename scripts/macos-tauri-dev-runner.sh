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
runner_directory="$project_directory/backend/target/development.noindex/macos-dev-runners"
mkdir -p "$runner_directory"

# 通知中心按应用身份解析启动位置，固定路径避免同一身份对应多个历史开发包。
# 产物保留在被 Git 和 Spotlight 忽略的开发目录，下一次启动原位更新。
app_bundle="$runner_directory/RelayPulse Dev.app"
lsregister=$(command -v lsregister || true)
if [ -z "$lsregister" ]; then
  lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
fi

contents_directory="$app_bundle/Contents"
executable_directory="$contents_directory/MacOS"
resources_directory="$contents_directory/Resources"
bundled_binary="$executable_directory/relaypulse"

mkdir -p "$executable_directory" "$resources_directory"
cp "$source_binary" "$bundled_binary"
cp "$script_directory/macos-dev-Info.plist" "$contents_directory/Info.plist"
cp "$project_directory/backend/icons/icon.icns" "$resources_directory/icon.icns"

if [ -d "$project_directory/backend/infoplist" ]; then
  cp -R "$project_directory/backend/infoplist/." "$resources_directory/"
fi

codesign --force --sign - --identifier com.relaypulse.app.dev "$bundled_binary"
codesign --force --deep --sign - --identifier com.relaypulse.app.dev "$app_bundle"

# 只注销本项目旧运行器留下的开发包，不改其他应用或正式版注册。
for previous_bundle in "$runner_directory"/relaypulse.*/"RelayPulse Dev.app"; do
  if [ -d "$previous_bundle" ]; then
    "$lsregister" -u "$previous_bundle" >/dev/null 2>&1 || true
  fi
done
"$lsregister" -f "$app_bundle"

# Tauri 直接持有应用 PID，热更新或关闭启动器时不会留下 shell 的孤儿子进程。
exec "$bundled_binary" "$@"
