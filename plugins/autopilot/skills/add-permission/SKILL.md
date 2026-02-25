---
name: add-permission
description: Use when a Bash command is denied and you cannot proceed without it. Helps propose a specific permission rule to add to the project's allow list.
---

# Add Permission

Propose a specific permission rule when a command is denied in autonomous mode.

## Permission Syntax

- `Bash(command)` — exact match (no arguments)
- `Bash(command *)` — command with any arguments
- `Bash(command specific-arg *)` — command with specific prefix

Always use `cmd` + `cmd *` pairs (never `cmd*`). Exception: `cmd:*` for heredoc commands (e.g., `git commit:*`).

## Security Principles

**Least privilege:** Allow the most specific pattern that covers the use case.

**Avoid bypass vectors:** Never propose rules that allow arbitrary code execution:
- `Bash(bun run *)` — use specific scripts instead: `Bash(bun run test)`, `Bash(bun run test *)`
- `Bash(python3 *)` — use specific entry points: `Bash(uv run scripts/train.py *)`
- `Bash(bash *)` — use specific scripts: `Bash(bash scripts/deploy.sh *)`

**Good examples:**
- `Bash(git stash)` + `Bash(git stash *)` — specific git operation
- `Bash(cargo test)` + `Bash(cargo test *)` — specific project script
- `Bash(curl https://api.example.com/*)` — specific API endpoint

## When to Edit Settings Directly

If the permission is obvious, safe, and follows least privilege — edit `.claude/settings.json` or `.claude/settings.local.json` directly. This triggers its own permission prompt, so the user sees and approves the proposed rule.

Check which file has the existing permissions and add to the appropriate one.

## When to Stop and Discuss

- The permission is ambiguous or could match unintended commands
- It could be a bypass vector (allows arbitrary code execution)
- The user might prefer a different approach entirely
- The denied command suggests a workflow problem (e.g., needing `bash -c` suggests the command should be a script instead)
