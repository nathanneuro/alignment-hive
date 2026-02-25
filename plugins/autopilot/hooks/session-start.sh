#!/bin/bash
set -euo pipefail

STATE_FILE="$CLAUDE_PROJECT_DIR/.claude/autopilot/state.json"
LOG_FILE="$HOME/.cache/autopilot/bootstrap.log"

# No state file → not configured
if [ ! -f "$STATE_FILE" ]; then
  echo '{"systemMessage": "autopilot: Not configured. Run /autopilot:setup"}'
  exit 0
fi

# Ensure jq is available for state check and auto-deny hook
mkdir -p "$(dirname "$LOG_FILE")"
bash "${CLAUDE_PLUGIN_ROOT}/scripts/ensure-jq.sh" 2>"$LOG_FILE"

if command -v jq >/dev/null 2>&1; then
  JQ="jq"
elif [ -x "$HOME/.cache/autopilot/jq" ]; then
  JQ="$HOME/.cache/autopilot/jq"
else
  echo "{\"systemMessage\": \"autopilot: Setup error, autonomous mode disabled. Details: $LOG_FILE\"}"
  exit 0
fi

# Check if autonomous mode is enabled
if ! "$JQ" -e '.autonomous_mode == true' "$STATE_FILE" >/dev/null 2>&1; then
  exit 0
fi
