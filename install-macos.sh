#!/bin/sh
# Installs Activity Monitor on macOS as a per-user launchd agent.
#
# Usage (from the unzipped release, which contains this script next to the
# three binaries and appsettings.json):
#   ./install-macos.sh            install + start the agent
#   ./install-macos.sh uninstall  stop + remove the agent
#
# Prereqs:
#   - Ollama for inference:  brew install ollama && ollama serve
#     (the agent only probes it; it never installs or starts anything)
#   - Accessibility permission for the agent binary:
#     System Settings > Privacy & Security > Accessibility
#     (macOS 27.3+: approve the prompt by selecting the app from the list,
#     or add it in the Accessibility pane; "ask every time" needs a re-approve
#     per prompt. A transparency indicator shows while the AX API is in use.)
#     Without it the agent records idle events only (no focus text).
set -eu

LABEL="com.greathavoc.activitymonitor"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
DIR="$HOME/Library/Application Support/ActivityMonitor"
LOGS="$HOME/Library/Logs/ActivityMonitor"
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

uninstall() {
    launchctl bootout "gui/$UID" "$PLIST" 2>/dev/null || true
    rm -f "$PLIST"
    echo "Agent uninstalled (data kept in: $DIR)"
}

if [ "${1:-}" = "uninstall" ]; then
    uninstall
    exit 0
fi

mkdir -p "$DIR" "$LOGS"
for f in activity-monitor-service activity-monitor-cli activity-monitor-mcp appsettings.json; do
    if [ ! -f "$SCRIPT_DIR/$f" ]; then
        echo "Missing $f next to install-macos.sh; run this from the unzipped release dir." >&2
        exit 1
    fi
    cp "$SCRIPT_DIR/$f" "$DIR/$f"
done

cat > "$PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>$LABEL</string>
    <key>ProgramArguments</key>
    <array>
        <string>$DIR/activity-monitor-service</string>
    </array>
    <key>WorkingDirectory</key>
    <string>$DIR</string>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>$LOGS/service.out.log</string>
    <key>StandardErrorPath</key>
    <string>$LOGS/service.err.log</string>
</dict>
</plist>
EOF

launchctl bootout "gui/$UID" "$PLIST" 2>/dev/null || true
launchctl bootstrap "gui/$UID" "$PLIST"

echo "Installed and started $LABEL"
echo "  binaries/config : $DIR"
echo "  database        : $DIR/Data/ActivityData.db"
echo "  logs            : $LOGS and $DIR/Logs"
echo "  stop/start      : launchctl stop/start $LABEL   |   ./install-macos.sh uninstall"
echo ""
echo "Two manual steps make full use of it:"
echo "  1. brew install ollama && ollama serve   (inference; model pulled per appsettings.json)"
echo "  2. Grant Accessibility to $DIR/activity-monitor-service (System Settings >"
echo "     Privacy & Security > Accessibility; on macOS 27.3+ approve the prompt by"
echo "     selecting the app from the list). Without it only idle events are recorded."
