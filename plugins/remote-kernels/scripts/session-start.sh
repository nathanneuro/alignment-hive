#!/bin/bash
# SessionStart hook: nudge users to set up remote-kernels if not configured.

# If remote-kernels.toml exists, setup is done — exit silently.
if [ -f "$CLAUDE_PROJECT_DIR/remote-kernels.toml" ]; then
  exit 0
fi

echo '{"systemMessage": "remote-kernels: Not configured. Run /remote-kernels:setup"}'
