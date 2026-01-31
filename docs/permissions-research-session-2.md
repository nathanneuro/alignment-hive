# Permissions & Safety Research - Session 2

Research session analyzing real tool usage patterns to inform permission recommendations.

## Data Collection

Created two scripts to analyze actual usage patterns:

- `scripts/aggregate-tool-calls.py` - Aggregates tool calls from session history
- `scripts/collect-permissions.py` - Collects permission settings from projects

Data saved to `docs/data/` (gitignored).

## Findings from Session History

**Scope:** 1,728 sessions across 77 projects, 24,731 tool calls

### Top Bash Commands by Frequency

| Command | Count | Notes |
|---------|-------|-------|
| bun | 1,835 | Package manager, build tool |
| git | 1,782 | Version control |
| cd | 790 | Directory navigation |
| ls | 789 | File listing |
| grep | 368 | Search (despite Grep tool existing) |
| cat | 297 | File reading (despite Read tool) |
| source | 238 | Shell sourcing |
| cargo | 205 | Rust toolchain |
| echo | 195 | Output/debugging |
| rm | 183 | File deletion |
| find | 141 | File search |
| curl | 137 | HTTP requests |
| head | 107 | File preview |
| tmux | 88 | Session management |
| uv | 87 | Python package manager |
| python3 | 83 | Python execution |
| jq | 82 | JSON processing |
| gh | 79 | GitHub CLI |
| mkdir | 67 | Directory creation |

### Top WebFetch Domains

| Domain | Count | Denial Rate | Notes |
|--------|-------|-------------|-------|
| code.claude.com | 137 | 76% | Claude Code docs (auth/redirect issues) |
| github.com | 40 | 20% | Code hosting |
| workos.com | 35 | 20% | Auth provider docs |
| raw.githubusercontent.com | 26 | 0% | Raw file access |
| docs.convex.dev | 18 | 28% | Convex documentation |
| clerk.com | 17 | 100% | Auth provider (blocked) |
| tanstack.com | 10 | 0% | TanStack docs |
| bun.sh | 7 | 0% | Bun documentation |

### Other Tool Usage

| Tool | Count | Denial Rate |
|------|-------|-------------|
| Read | 4,931 | 8% |
| Edit | 4,481 | 1% |
| Grep | 1,163 | 1% |
| Write | 690 | 4% |
| Glob | 624 | 0% |
| Task | 588 | 9% |
| WebSearch | 338 | 21% |

## Findings from Existing Permissions

**Scope:** 54 settings files, 552 allow rules, 1 deny rule

### Most Common Allow Patterns

**Bash commands:**
- `git add:*` (15 projects)
- `mkdir:*` (14 projects)
- `grep:*` (12 projects)
- `rg:*` (11 projects)
- `pnpm run:*` (11 projects)
- `git commit:*` (9 projects)
- `cargo clippy:*` (8 projects)
- `cargo fmt:*` (6 projects)

**WebFetch domains:**
- `domain:daisyui.com` (12 projects)
- `domain:docs.convex.dev` (11 projects)
- `domain:tanstack.com` (10 projects)
- `domain:github.com` (8 projects)
- `domain:zod.dev` (7 projects)

**MCP tools:**
- `mcp__ide__getDiagnostics` (11 projects)
- `mcp__playwright` (7 projects)
- `mcp__llms-fetch__fetch` (4 projects)

**Only deny rule:** `Bash(timeout:*)` in global settings

---

## User Preferences & Design Decisions

### WebFetch Tiers

Three tiers based on sensitivity of work:

1. **Tier 1 (least restrictive):** Allow llms-fetch-mcp + WebFetch from all domains
   - For users working on non-sensitive projects
   - Less concerned with prompt injection risk

2. **Tier 2:** Allow just WebFetch from all domains
   - WebFetch has built-in prompt injection protections
   - Read-only, fairly low risk

3. **Tier 3 (most restrictive):** Allow WebFetch from particular domains only
   - For sensitive work
   - Limit to documentation sites

### Bash Commands Approach

- Recommend allowing non-destructive commands as a **bundle** (don't ask user one-by-one, just show examples)
- **Prioritize mid-session commands** - commands needed for verification (compiling, linting, running experiments) are critical to allow
- Commands at session start/end can be more flexible
- Let user choose whether to allow commits or leave in "ask" mode

### Risk Appetite Framing

- Risk isn't just about security - **also about Claude going down unproductive paths** / not getting pushed back when using wrong commands
- Example: User's `timeout` deny exists because it's not installed - prevents Claude from waiting for permission to run a command that will fail anyway

### Additional Settings to Recommend

- `additionalDirectories` - if user has a global docs folder or folder for cloning third-party code
- `defaultMode: "plan"` - recommended for most users

### curl Approach

- Same tier as llms-fetch-mcp (Tier 1)
- Deny POST/PUT/DELETE requests
- Rationale: If Claude wants to send POST requests, writing a script in /tmp/claude-scripts is healthier (more explicit, user can review)

### Git/gh Permissions (Deferred)

- If allowing `git:*`, need comprehensive and thoughtful deny rules
- Don't include in most restrictive tier
- Decision deferred to destructive commands discussion

---

## Permission System Experiments (Session 2 continued)

### Experiment 1: Deny Rule Priority

**Question:** Can we use `deny: ["Bash(*)"]` as a default and override with specific allow rules?

**Test:**
- Added `deny: ["Bash(*)"]` to settings.local.json
- Tested commands in allow list (`ls`) and commands covered by acceptEdits static analysis

**Results:**
| Test | Expected | Actual |
|------|----------|--------|
| `echo` (not in allow list) | Denied | ✓ Denied |
| `ls` (in allow list) | Allowed | ✗ Denied |
| `touch ./file` (acceptEdits scope) | Allowed | ✗ Denied |

**Conclusion:** Deny takes absolute priority over both allow rules and acceptEdits static analysis. Cannot use blanket deny as default.

---

### Experiment 2: /tmp/claude-scripts Pattern

**Question:** Can we create a permission boundary for one-off scripts that prompts once per session?

**Setup:**
- Allow: `Bash(python3 /tmp/claude-scripts/*)`
- No Write permission for /tmp

**Test:**
1. Write script to /tmp/claude-scripts/ → Prompted for session access ✓
2. Run script → Allowed (bash permission matched) ✓
3. Edit script → No prompt (session access already granted) ✓
4. Run in subdirectory → Allowed (single `*` works recursively) ✓

**Conclusion:** Pattern works as intended. User gets prompted once per session when Claude first writes to /tmp/claude-scripts, then can work autonomously. **User explicitly liked this pattern.**

---

### Experiment 3: Selective Deny for Dangerous Subcommands

**Question:** Can we allow a command broadly while denying specific dangerous variants?

**Setup:**
- Allow: `Bash(find:*)`
- Deny: `Bash(find*-exec*)`

**Test:**
| Command | Result |
|---------|--------|
| `find docs -name "*.md"` | ✓ Allowed |
| `find docs -name "*.md" -exec echo {} \;` | ✓ Denied |
| `find docs -name "*.md" -execdir echo {} \;` | ✓ Denied |

**Conclusion:** Deny patterns can match substrings. Pattern `find*-exec*` blocks both `-exec` and `-execdir`. This enables safe allow + deny combinations. **User explicitly liked this pattern.**

---

### Experiment 4: env Command for Environment Variables

**Question:** Can `Bash(env:*)` allow per-command env vars?

**Test:** `env FOO=bar cat /etc/hosts` (cat not in allow list)

**Result:** Command ran successfully - `cat` executed despite not being in allow list.

**Conclusion:** `env:*` is a security bypass. The `env` command syntax is `env [VAR=value]... COMMAND`, so allowing `env:*` effectively allows any command. **Rejected.**

---

### Experiment 5: export + && Chaining

**Question:** Does Claude Code validate each part of a `&&` chain separately?

**Setup:**
- Allow: `Bash(export:*)`
- Allow: `Bash(ls:*)`
- Do NOT allow: `echo`

**Test 1:** `export FOO=bar && echo test`
- Result: Prompted for `echo` permission → Claude Code validates each part separately ✓

**Test 2:** `export FOO=bar && ls docs/`
- Result: Ran successfully, no prompt ✓

**Test 3:** `export TEST_VAR=hello && python3 /tmp/claude-scripts/print-env.py`
- Script reads `os.environ.get('TEST_VAR')`
- Result: Printed `TEST_VAR = hello` ✓

**Conclusion:** `export:*` is safe. Claude Code validates each `&&`-chained command separately. Environment variables set by export ARE available to subsequent commands in the chain. This is the solution for per-command env vars. **User explicitly liked this.**

---

## Decisions Made

### Accepted Patterns

1. **`/tmp/claude-scripts/*`** - One-off script location with session-scoped write access
2. **`Bash(find:*)` + `Bash(find*-exec*)` deny** - Allow find, block dangerous -exec
3. **`Bash(export:*)`** - Safe way to set env vars for subsequent commands via `&&` chaining
4. **Selective deny patterns** - Can block dangerous subcommands while allowing base command

### Rejected Approaches

1. **`Bash(*)` deny as default** - Deny takes absolute priority, blocks everything
2. **`Bash(env:*)`** - Security bypass, allows arbitrary command execution
3. **xargs** - Same risk as find -exec, not worth the complexity

### Deferred Decisions

1. **Git permissions** - Defer to destructive commands discussion (allow git:* with specific denies?)
2. **GitHub CLI** - Same approach as git
3. **curl** - Allow with deny patterns for POST/PUT/DELETE

---

## Non-Destructive Commands Bundle (Draft)

Commands to recommend allowing in the base tier:

### File System Inspection
- `Bash(ls:*)`, `Bash(find:*)` (with -exec deny), `Bash(cat:*)`, `Bash(head:*)`, `Bash(tail:*)`
- `Bash(wc:*)`, `Bash(file:*)`, `Bash(stat:*)`, `Bash(du:*)`, `Bash(df:*)`
- `Bash(diff:*)`, `Bash(tree:*)`, `Bash(realpath:*)`

### Text Processing
- `Bash(grep:*)`, `Bash(jq:*)`, `Bash(awk:*)`, `Bash(sort:*)`, `Bash(uniq:*)`, `Bash(cut:*)`, `Bash(tr:*)`

### System Inspection
- `Bash(echo:*)`, `Bash(pwd:*)`, `Bash(which:*)`, `Bash(type:*)`, `Bash(env:*)` (read-only, just prints)
- `Bash(uname:*)`, `Bash(whoami:*)`, `Bash(date:*)`, `Bash(ps:*)`, `Bash(pgrep:*)`

### Utility
- `Bash(sleep:*)`, `Bash(export:*)`

### Control Flow (for loops)
- `Bash(for:*)`, `Bash(do)`, `Bash(done)` - safe, loop body validated separately

### Paired Deny Rules
- `Bash(find*-exec*)`

---

### Experiment 6: For Loops (`for:*`, `do`, `done`)

**Question:** Can `Bash(for:*)` be used to bypass permissions for commands inside loops?

**Setup:**
- Allow: `Bash(for:*)`, `Bash(do)`, `Bash(done)`
- Allow: `Bash(grep:*)`
- Do NOT allow: `echo`, `rm`

**Test 1: Simple for loop with allowed command in body**
```bash
for i in a b c; do
grep --version
done
```
- Result: Ran successfully, no prompt ✓

**Test 2: For loop with disallowed command in body**
```bash
for i in 1 2 3; do
echo $i
done
```
- Result: Prompted for `echo` → Claude Code validates loop body separately ✓

**Test 3: Command substitution in for list**
```bash
for i in `echo test`; do
grep --version
done
```
- Result: Generic yes/no prompt (parser gives up on backticks)

**Test 4: Various command substitution forms**
- `$(command)` syntax → Generic prompt
- Backticks → Generic prompt
- `$(< file)` → Generic prompt
- C-style `for ((i=0; i<3; i++))` → Generic prompt

**Test 5: Simple variable expansion**
```bash
echo $HOME
```
- Result: Works fine, properly parsed ✓

**Conclusion:**
1. `for:*` + `do` + `done` is **SAFE** - loop body commands are validated separately
2. Any form of command substitution (`$()`, backticks, `$(< file)`) causes the parser to give up → falls back to generic yes/no prompt
3. Simple variable expansion (`$VAR`) works fine
4. No security bypasses found

**Implication:** Can recommend allowing `for:*`, `do`, `done` for users who need bash loops. Should also add guidance to prefer simple commands and avoid command substitution when possible (to get proper permission parsing).

---

### Experiment 7: Directory Access Prompt Bug (rm + glob)

**Observation:** When running `rm ./pattern-*`, user gets prompted "allow access to agile-glow-5" even though already in acceptEdits mode.

**Investigation:**

| Command | Prompt? | Notes |
|---------|---------|-------|
| `touch ./test-file` | No | Simple touch works |
| `rm ./test-file` | No | Simple rm works |
| `ls ./docs/*.md` | No | ls with glob works |
| `ls ./nonexistent-*` | No | ls with non-matching glob works |
| `rm -f ./nonexistent-*` | **YES** | Prompts for directory access |
| `rm ./glob-test-*` | **YES** | Prompts for directory access |
| `rm ./glob-test-1 ./glob-test-2` | No | Explicit filenames work |

**Conclusion:** Bug is specifically the combination of `rm` + glob pattern. Other commands with globs work fine. `rm` with explicit filenames works fine.

**Prompt shown:** "Yes, and always allow access to agile-glow-5/ from this project"

**Status:** Bug to report to Claude Code team.

---

## Other Discoveries

### Claude Self-Edit Option

When Claude attempts to edit settings.local.json, the permission prompt includes an option: "Yes, and allow Claude to edit its own settings for this session". Could be useful for dynamic permission workflows where Claude adds permissions as needed.

---

## Next Steps

1. **Destructive commands bundle** - Define what to deny (rm -rf, git push --force, etc.)
2. **Git/gh permissions** - Decide on allow git:* with specific denies, or enumerate safe commands
3. **curl with deny patterns** - Test `Bash(curl*-X POST*)`, `Bash(curl*--data*)`
4. **WebFetch tiers** - Document the three levels (llms-fetch + all, WebFetch all, specific domains)
5. **Design skill interaction flow** - How many questions, what order, how to present tiers
6. **Additional recommendations** - `additionalDirectories`, `defaultMode: "plan"`
7. **Investigate directory access bug** - Why does acceptEdits mode prompt for directory access?
8. **Command substitution guidance** - Add to CLAUDE.md recommendations to prefer simple commands

---

## Session Metadata

- Date: 2026-01-30
- Focus: Empirical analysis of tool usage for permission generator design
- Outcome: Data collected, patterns identified, experiments run, key patterns validated
