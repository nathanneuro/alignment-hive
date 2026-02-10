# Permissions Design - Session 4

CLAUDE.md recommendations for the permissions skill.

## Overview

This session focused on xargs experiments and designing the CLAUDE.md content that the permissions skill will generate. Key findings: xargs is reliable with proper allow rules, for loops have persistent bugs and should be denied, and CLAUDE.md guidance should be minimal and actionable.

## xargs Experiments

### Internal Whitelist Discovery

Claude Code has an internal xargs whitelist for file-reading commands. This whitelist is **separate from user settings** and works automatically without explicit allow rules.

**Commands in the internal xargs whitelist:** grep, head, tail, wc, echo

**Commands NOT in the whitelist (prompt even if the command itself is allowed):** cat, ls, file, sed, awk, stat, du, sort, cut, tr, diff, basename, dirname

The whitelist appears to be related to common file-reading operations that are frequently piped.

### Behavior Before Explicit Allow Rules

**xargs with whitelisted commands (no allow rules needed):**
- `ls *.md | xargs wc -l` ✅ Works
- `ls *.md | xargs grep "pattern"` ✅ Works
- `ls *.md | xargs head -1` ✅ Works

**xargs with non-whitelisted commands (before adding allow rules):**
- `ls *.md | xargs cat` → Prompts for "xargs cat"
- `ls *.md | xargs awk 'NR==1'` → Prompts for "xargs awk"
- `ls *.md | xargs sed -n '1p'` → Prompts for "xargs" (different prompt text)
- `ls *.md | xargs file` → Prompts for "xargs file"
- `ls *.md | xargs stat` → Prompts for "xargs stat"

**sed standalone behavior (before adding to allow list):**
- `sed -n '1p' file` → Prompts for "allow access to docs/ from this project"
- User approves → command runs
- `sed -n '2p' file` (next command) → Generic yes/no prompt (no "always allow" option)
- Directory access permission didn't persist

**awk standalone behavior:**
- `awk 'NR==1' file` → Worked after adding `Bash(awk *)` to allow list

### Explicit Allow Rules Augment the Whitelist

Explicit allow rules work alongside the internal whitelist. The pattern syntax matters:

| Pattern | Works? | Notes |
|---------|--------|-------|
| `Bash(xargs cat *)` | ❌ | Space before `*` doesn't match piped commands |
| `Bash(xargs cat*)` | ✅ | No space before `*` works |
| `Bash(xargs -I{} cat*)` | ✅ | Works for positional replacement |

**Tested commands with explicit `Bash(xargs <cmd>*)` rules:**
- `ls *.md | xargs cat` ✅
- `ls *.md | xargs awk 'NR==1'` ✅
- `ls *.md | xargs sed -n '1p'` ✅
- `ls *.md | xargs file` ✅
- `ls *.md | xargs stat` ✅
- `ls *.md | xargs sort` ✅
- `ls *.md | xargs du -h` ✅

### xargs -I{} Experiments

The `-I{}` flag for positional replacement works with explicit allow rules:

| Test | Result |
|------|--------|
| `ls \| xargs -I{} cat {}` | ✅ Works with `Bash(xargs -I{} cat*)` |
| `ls \| xargs -I{} head -1 {}` | ✅ Works |
| `ls \| xargs -I{} echo "File: {}"` | ✅ Works |
| `ls \| xargs -I{} uv run script.py {}` | ✅ Works with specific script allow |
| Multiple `{}` in command | ✅ Works: `xargs -I{} echo "{} has path {}"` |

### Deny Rules for Steering

Added deny rules to steer away from shell execution patterns:

| Pattern | Purpose |
|---------|---------|
| `Bash(xargs sh *)` | Deny `xargs sh -c` - use scripts instead |
| `Bash(xargs -I{} sh *)` | Deny `xargs -I{} sh -c` - use scripts instead |

**Tested:**
- `echo "test" | xargs sh -c 'echo hello'` → Permission denied ✅
- `ls | xargs -I{} sh -c 'cat {}'` → Permission denied ✅

### For Loop Experiments

**For loops with file-reading commands - ALL trigger directory access prompt:**

Using multiline syntax (with `do` on its own line):

```bash
for f in *.md
do
cat "$f"
done
```

Commands tested in loop body that trigger directory access prompt: cat, wc, head, awk, sed, file, sort, cut, diff

**Key observation:** Even after approving directory access, the permission doesn't persist. The next command prompts for directory access again (unlike standalone sed which fell back to generic yes/no).

**For loop with sed (after adding Bash(sed *) to allow list):**

```bash
for f in README.md
do
sed -n '1p' "$f"
done
```

- First run → Prompts for "allow access to project/"
- User approves → command runs
- Same command again → Prompts for "allow access to project/" again
- Permission never persists, never falls to generic

**Bug to report:** Claude Code sometimes misidentifies why it's rejecting a command, displaying an "always allow" prompt for the wrong reason (e.g., directory access when the actual issue is command parsing or the command not being allowed). Because it's not the real reason, the permission doesn't persist. This affects standalone sed (before adding to allow list), for loops with file-reading commands, rm + glob (from session 2), and potentially other cases.

**For loops with non-file-reading commands:**

Using multiline syntax (with `do` on its own line):

- echo: ✅ Works (in allow list from session 3)
- export: ✅ Works (in allow list from session 3)
- printf: ✅ Works after adding `Bash(printf *)`
- basename: ✅ Works after adding `Bash(basename *)`
- dirname: ✅ Works after adding `Bash(dirname *)`

**Conclusion:** The for loop bug specifically affects file-reading operations. Non-file-reading commands in for loops work correctly, but require explicit allow rules.

### Edge Case Tests

| Test | Result |
|------|--------|
| `export VAR=val && command` | ✅ Works |
| `export VAR=val && ls \| xargs -I{} cmd {}` | ✅ Works (env inherited) |
| Pipes after xargs: `ls \| xargs cat \| grep pattern` | ✅ Works |
| `&&` before xargs: `ls file && ls \| xargs head` | ✅ Works |
| Different env var per invocation | ❌ Not possible with xargs, use scripts |

---

## Updated Permission Recommendations

Based on the experiments above, here are the corrected permission patterns.

### xargs Allow Rules

**Pattern syntax clarification:**
- `Bash(xargs cmd*)` (no space) - matches `xargs cmd` with no explicit args (args come from pipe)
- `Bash(xargs cmd *)` (with space) - matches `xargs cmd arg...` where explicit args are always present

**Commands often used without explicit args (no space before `*`):**

```
Bash(xargs cat*)
Bash(xargs file*)
Bash(xargs head*)
Bash(xargs tail*)
Bash(xargs wc*)
Bash(xargs stat*)
Bash(xargs sort*)
Bash(xargs du*)
Bash(xargs diff*)
Bash(xargs basename*)
Bash(xargs dirname*)
```

**Commands that always have explicit args (space before `*`):**

```
Bash(xargs awk *)
Bash(xargs sed *)
Bash(xargs grep *)
Bash(xargs cut *)
```

**xargs -I{} variants (always have `{}` arg, use space):**

Note: Original experiments tested `Bash(xargs -I{} cat*)` (no space), but since -I{} always has at least the `{}` argument, using space (`Bash(xargs -I{} cat *)`) is more precise and also works.

```
Bash(xargs -I{} cat *)
Bash(xargs -I{} file *)
Bash(xargs -I{} head *)
Bash(xargs -I{} tail *)
Bash(xargs -I{} wc *)
Bash(xargs -I{} stat *)
Bash(xargs -I{} sort *)
Bash(xargs -I{} du *)
Bash(xargs -I{} diff *)
Bash(xargs -I{} basename *)
Bash(xargs -I{} dirname *)
Bash(xargs -I{} awk *)
Bash(xargs -I{} sed *)
Bash(xargs -I{} grep *)
Bash(xargs -I{} cut *)
```

### xargs with Scripts (Tier-Specific)

**Tier 1 (local claude-execution-allowed/):**

```
# Plain xargs (all files as args)
Bash(xargs bun run claude-execution-allowed/*)
Bash(xargs uv run claude-execution-allowed/*)
Bash(xargs bash claude-execution-allowed/*)

# xargs -I{} (one file at a time)
Bash(xargs -I{} bun run claude-execution-allowed/*)
Bash(xargs -I{} uv run claude-execution-allowed/*)
Bash(xargs -I{} bash claude-execution-allowed/*)
```

**Tier 2 (/tmp scripting):**

```
# Plain xargs (all files as args)
Bash(xargs bun /tmp/claude-execution-allowed/*)
Bash(xargs uv run /tmp/claude-execution-allowed/*)
Bash(xargs bash /tmp/claude-execution-allowed/*)

# xargs -I{} (one file at a time)
Bash(xargs -I{} bun /tmp/claude-execution-allowed/*)
Bash(xargs -I{} uv run /tmp/claude-execution-allowed/*)
Bash(xargs -I{} bash /tmp/claude-execution-allowed/*)
```

### xargs with Blessed Scripts (Project-Specific)

When enumerating package.json scripts or project scripts, also add xargs variants if potentially relevant for bulk operations:

```
# Example for a project with analyze.py script
Bash(uv run scripts/analyze.py *)
Bash(xargs uv run scripts/analyze.py *)
Bash(xargs -I{} uv run scripts/analyze.py *)

# Example for npm project with process script
Bash(bun run process *)
Bash(xargs bun run process *)
Bash(xargs -I{} bun run process *)
```

Not all scripts need xargs variants - use judgment based on whether the script is likely to be used for bulk file processing.

### xargs Deny Rules

```
Bash(xargs sh *)
Bash(xargs -I{} sh *)
Bash(xargs bash *)
Bash(xargs -I{} bash *)
```

### Loop Deny Rules

For loops have a persistent bug with file-reading commands and aren't useful enough to warrant the complexity. Deny all loop constructs - use scripts instead.

```
Bash(for *)
Bash(while *)
Bash(until *)
```

**Correction to Session 3:** Remove `Bash(for *)`, `Bash(do)`, `Bash(done)` from allow list. Remove `Bash(do *)` from deny list (no longer needed since loops are denied entirely).

---

## Design Principles

1. **Instructions are for Claude, not users** - Frame everything as what would help Claude navigate permissions effectively
2. **Minimal context** - Keep CLAUDE.md brief; the skill handles complexity
3. **Point to the skill for changes** - Don't explain how to update permissions, just when to invoke the skill
4. **Let the permission system work** - Don't document things handled well by allow/deny rules (protected files, git prompts, web domains)

## What NOT to Include

Things we considered but decided against:

- **Protected files (.env)** - Deny rules handle this; Claude will see denials naturally
- **Git commit permissions** - System handles correctly (allowed or prompted); users who want autonomous commits will have separate instructions
- **Web access tiers** - Prompts are fine; most users allow all domains anyway
- **Non-interactive (-p) mode specifics** - Claude gets clear denial messages; no special documentation needed
- **Repeated prompt detection** - Claude doesn't see prompts, only denials

## CLAUDE.md Sections

### Section 1: Package Manager Commands

**Trigger:** Always included (adapted to detected package manager)

```markdown
## Permissions

[Package manager] scripts from [package.json/pyproject.toml] are pre-allowed.
```

Brief context that blessed commands exist. No need to list all safe commands (ls, cat, etc.) since Claude uses them naturally.

---

### Section 2: Bash Operations

**Trigger:** Always included, content varies by ad-hoc script tier

Three tiers based on user's script execution preference.

**Implementation note:** Most content is shared across tiers. The skill should use a shared template with tier-specific variables for:
- Script location paragraph (where to put scripts, or "avoid/ask user")
- Last two patterns ("script, not ..." vs "avoid ...")

#### Tier 1: Scripts Always Allowed (local `claude-execution-allowed/`)

```markdown
**Bash operations:**

Complex bash syntax is hard for Claude Code to permission correctly. Keep commands simple.

Simple operations are fine: `|`, `||`, `&&`, `>` redirects.

For bulk operations on multiple files, use xargs:
- Plain: `ls *.md | xargs wc -l`
- With placeholder: `ls *.md | xargs -I{} head -1 {}`

For string interpolation (`$()`, backticks, `${}`), heredocs, loops, or advanced xargs flags (`-P`, `-L`, `-n`), write a script in `claude-execution-allowed/` instead. Run via `[bun run/uv run/bash] claude-execution-allowed/script-name`.

**Patterns:**
- File creation: Write tool, not `cat << 'EOF' > file`
- Env vars: `export VAR=val && command`, not `VAR=val command` or `env VAR=val command`
- Bulk operations: `ls *.md | xargs wc -l`, not `for f in *.md; do cmd "$f"; done`
- Parallel/batched xargs: script, not `xargs -P4` or `xargs -L1`
- Per-item shell logic: script, not `xargs sh -c '...'`
```

#### Tier 2: /tmp Scripting Allowed

```markdown
**Bash operations:**

Complex bash syntax is hard for Claude Code to permission correctly. Keep commands simple.

Simple operations are fine: `|`, `||`, `&&`, `>` redirects.

For bulk operations on multiple files, use xargs:
- Plain: `ls *.md | xargs wc -l`
- With placeholder: `ls *.md | xargs -I{} head -1 {}`

For string interpolation (`$()`, backticks, `${}`), heredocs, loops, or advanced xargs flags, write a script in `/tmp/claude-execution-allowed/[project]/` instead.

**Patterns:**
- File creation: Write tool, not `cat << 'EOF' > file`
- Env vars: `export VAR=val && command`, not `VAR=val command` or `env VAR=val command`
- Bulk operations: `ls *.md | xargs wc -l`, not `for f in *.md; do cmd "$f"; done`
- Parallel/batched xargs: script, not `xargs -P4` or `xargs -L1`
- Per-item shell logic: script, not `xargs sh -c '...'`
```

#### Tier 3/4: No Scripting Allowed

```markdown
**Bash operations:**

Complex bash syntax is hard for Claude Code to permission correctly. Keep commands simple.

Simple operations are fine: `|`, `||`, `&&`, `>` redirects.

For bulk operations on multiple files, use xargs:
- Plain: `ls *.md | xargs wc -l`
- With placeholder: `ls *.md | xargs -I{} head -1 {}`

Avoid string interpolation (`$()`, backticks, `${}`), heredocs, loops, and advanced xargs flags — break into simpler sequential commands or ask the user for help.

**Patterns:**
- File creation: Write tool, not `cat << 'EOF' > file`
- Env vars: `export VAR=val && command`, not `VAR=val command` or `env VAR=val command`
- Bulk operations: `ls *.md | xargs wc -l`, not `for f in *.md; do cmd "$f"; done`
- Parallel/batched xargs: avoid `xargs -P4` or `xargs -L1`
- Per-item shell logic: avoid `xargs sh -c '...'`
```

---

### Section 3: When to Update Permissions

**Trigger:** Always included

```markdown
**If a command that should be allowed is denied**, or if the project structure has changed (new package manager, new script patterns), ask the user if they'd like to run `/permissions` to update settings.
```

---

## Technical Notes

### Why xargs instead of for loops?

For loops have a persistent bug with file-reading commands (directory access prompts that don't persist). Rather than maintain complex guidance about when loops work vs don't, we deny loops entirely and use xargs for bulk operations.

xargs works reliably with:
- Plain form: `ls *.md | xargs cmd`
- Placeholder form: `ls *.md | xargs -I{} cmd {}`

For anything more complex (parallel execution with `-P`, line limits with `-L`, etc.), use scripts.

### Why avoid interpolation/heredocs?

Claude Code's bash parser gives up on complex syntax and falls back to generic yes/no prompts (which users see, not Claude). By avoiding these patterns, commands get proper permission matching.

### Why allow xargs in Tier 1?

Even though scripts are always allowed in Tier 1, xargs is reliable enough with proper permissions and often simpler for basic bulk operations. Scripts are still preferred for complex logic (parallel execution, per-item shell commands, etc.).

---

## Skill Architecture (To Design Next Session)

The permissions skill will likely be separate from `/best-practices`:

- `/best-practices` - General project setup, includes note to run `/permissions` for permission configuration
- `/permissions` - Dedicated skill for permission setup and updates

The `/permissions` skill needs to handle:
1. **Initial setup** - Interactive questions, generate settings.json + CLAUDE.md
2. **Updates** - When project structure changes or new patterns needed

---

## Session Metadata

- Date: 2026-02-01
- Focus: xargs experiments, CLAUDE.md design, corrected permission recommendations
- Outcome: xargs allow/deny patterns finalized, loops denied entirely, CLAUDE.md tiers defined, ready to implement skill
