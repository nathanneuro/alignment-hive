#!/bin/bash

input=$(cat)

LOG_FILE="$HOME/.cache/autopilot/auto-deny-error.log"
mkdir -p "$(dirname "$LOG_FILE")"

# On any error, log it and tell the user
trap 'echo "$0: line $LINENO: unexpected error" >> "$LOG_FILE" 2>/dev/null; echo "{\"systemMessage\":\"autopilot: Hook error, autonomous mode disabled. Details: '$LOG_FILE'\"}"' ERR
set -euo pipefail

# Find jq: prefer global, fall back to bootstrapped
if command -v jq >/dev/null 2>&1; then
  JQ="jq"
elif [ -x "$HOME/.cache/autopilot/jq" ]; then
  JQ="$HOME/.cache/autopilot/jq"
else
  # No jq available — can't safely parse input, let normal prompt through
  exit 0
fi

# Extract everything we need in a single jq call
eval "$(echo "$input" | "$JQ" -r '
  "permission_mode=" + (.permission_mode | @sh),
  "has_session_dest=" + ([.permission_suggestions // [] | .[] | select(.destination == "session")] | length | tostring),
  "rule_content=" + ([.permission_suggestions // [] | .[] | select(.type == "addRules") | .rules[]? | .ruleContent] | first // "" | @sh),
  "has_suggestions=" + (.permission_suggestions // [] | length > 0 | tostring)
')"

if [ "$permission_mode" != "acceptEdits" ]; then
  exit 0
fi

# Check state file
STATE_FILE="$CLAUDE_PROJECT_DIR/.claude/autopilot/state.json"
if ! "$JQ" -e '.autonomous_mode == true' "$STATE_FILE" >/dev/null 2>&1; then
  exit 0
fi

# Let through suggestions with destination:"session" (directory grants, settings edits)
if [ "$has_session_dest" -gt 0 ]; then
  exit 0
fi

# Build context-aware deny message
if [ -n "$rule_content" ]; then
  message="Command denied in autonomous mode. \`${rule_content}\` is not in the allow list."
elif [ "$has_suggestions" = "true" ]; then
  message="Command denied in autonomous mode."
else
  message="Command denied in autonomous mode. Command likely contains command substitution or ambiguous syntax."
fi

# Output deny decision using jq to ensure valid JSON
"$JQ" -n --arg msg "$message" '{
  hookSpecificOutput: {
    hookEventName: "PermissionRequest",
    decision: {
      behavior: "deny",
      message: $msg,
      interrupt: false
    }
  }
}'
