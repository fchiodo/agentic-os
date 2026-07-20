#!/bin/sh

set -eu

SOURCE_APP="src-tauri/target/release/bundle/macos/Agent Control.app"
DESTINATION_ROOT="${AGENT_CONTROL_INSTALL_DIR:-$HOME/Applications}"
DESTINATION_APP="$DESTINATION_ROOT/Agent Control.app"

if [ ! -d "$SOURCE_APP" ]; then
  echo "Source app bundle not found: $SOURCE_APP" >&2
  exit 1
fi

mkdir -p "$DESTINATION_ROOT"
rsync -a --delete "$SOURCE_APP/" "$DESTINATION_APP/"
touch "$DESTINATION_APP"

echo "Synced Agent Control.app to $DESTINATION_APP"
