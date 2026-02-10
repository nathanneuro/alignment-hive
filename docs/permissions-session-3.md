# Permissions Design - Session 3

Permission configuration examples and patterns for the best-practices skill.

## Key Principles

**ALLOW**: Safe operations and blessed workflow commands. Be thoughtful about any command not explicitly validated.

**DENY**: For steering toward blessed alternatives, NOT for dangerous commands. Dangerous commands should use default ask behavior. Ask is safer than deny for prompt injection defense (deny gives the model another chance to find a bypass).

**ASK**: Only needed when a broad allow has dangerous subcommands (e.g., `find *` needs `find*-exec*` ask).

**Syntax** (from Claude Code docs):
- `Bash(npm run build)` - exact command
- `Bash(npm run test *)` - prefix with word boundary (space before `*`)
- `Bash(npm*)` - prefix without word boundary
- `Bash(* --help)` - suffix matching
- Legacy `:*` syntax is deprecated, use `*` instead
- Claude Code is aware of `&&`, `||`, `|` - prefix rules won't allow chained commands

---

## Permission Sets

### 1. Universal Safe Commands

**Trigger:** Always included for all projects.

```
# --- File System Inspection ---
Bash(ls *)
Bash(find *)                   # paired with find*-exec* ask
Bash(cat *)
Bash(head *)
Bash(tail *)
Bash(wc *)
Bash(file *)
Bash(stat *)
Bash(du *)
Bash(df *)
Bash(diff *)
Bash(tree *)
Bash(realpath *)
Bash(basename *)
Bash(dirname *)
Bash(mkdir *)

# --- Text Processing ---
Bash(grep *)
Bash(jq *)
Bash(awk *)
Bash(sort *)
Bash(uniq *)
Bash(cut *)
Bash(tr *)
Bash(sed *)
Bash(md5sum *)
Bash(sha256sum *)
Bash(base64 *)

# --- System Inspection ---
Bash(echo *)
Bash(pwd)
Bash(which *)
Bash(type *)
Bash(uname *)
Bash(whoami)
Bash(date *)
Bash(ps *)
Bash(pgrep *)
Bash(nvidia-smi *)
Bash(printenv *)
Bash(id)
Bash(hostname)
Bash(uptime)

# --- Utility ---
Bash(sleep *)
Bash(export *)                 # safe - && chained commands validated separately

# --- Control Flow ---
Bash(for *)
Bash(do)
Bash(done)

# --- Git (read-only) ---
Bash(git status *)
Bash(git diff *)
Bash(git log *)
Bash(git show *)
Bash(git branch)               # no wildcard - list only
Bash(git remote)               # no wildcard - list only
Bash(git remote -v)

# --- Local API Testing (for projects with web servers) ---
Bash(curl*localhost*)
Bash(curl*127.0.0.1*)
Bash(curl*0.0.0.0*)
```

### 2. Universal Ask Patterns

**Trigger:** Always included when `find *` is allowed.

```
Bash(find*-exec*)              # -exec runs arbitrary commands
```

### 3. Universal Deny Patterns

**Trigger:** Always included.

```
Bash(do *)                     # forces multiline loop syntax
Bash(timeout *)                # Bash tool has built-in timeout parameter
```

### 4. Never Allow (Open Set)

**Trigger:** These must never be added to allow lists. This is NOT exhaustive - be thoughtful about any command that could execute arbitrary input.

```
Bash(env *)                    # env VAR=val COMMAND runs any command
Bash(xargs *)                  # pipes input to any command
Bash(bash -c *)                # executes string as bash
Bash(sh -c *)                  # executes string as shell
Bash(eval *)                   # evaluates arbitrary code
Bash(time *)                   # time COMMAND runs any command
Bash(timeout *)                # timeout N COMMAND runs any command (also: Bash tool has built-in timeout)
```

---

## Project-Specific Permission Sets

### npm/bun Projects

**Trigger:** `package.json` detected. Detect package manager from lock file: `bun.lockb` → bun, `pnpm-lock.yaml` → pnpm, `yarn.lock` → yarn, `package-lock.json` → npm.

**Pattern:** Enumerate scripts from package.json. Example for bun:

```
# ALLOW - enumerated from package.json scripts
Bash(bun run dev *)
Bash(bun run build *)
Bash(bun run lint *)
Bash(bun run format *)
Bash(bun run test *)
Bash(bun run typecheck *)
Bash(bun test *)

# DENY - steer toward package manager
Bash(eslint *)
Bash(prettier *)
Bash(tsc *)
Bash(jest *)
Bash(vitest *)
Bash(node *)
Bash(npx *)
Bash(npm *)
Bash(pnpm *)
Bash(yarn *)
```

**Open question:** Does `Bash(bun run --filter *)` allow arbitrary script execution? Needs testing.

### uv/Python Projects

**Trigger:** `pyproject.toml` with `uv.lock` detected.

**Pattern:** Allow blessed tools via uv, enumerate known scripts. Example:

```
# ALLOW - testing/linting
Bash(uv run pytest *)
Bash(uv run mypy *)
Bash(uv run ruff *)

# ALLOW - enumerated project scripts
Bash(uv run scripts/analyze-results.py *)
Bash(uv run scripts/plot-metrics.py *)
Bash(uv run src/train.py *)
Bash(uv run src/eval.py *)

# DENY - steer toward uv run
Bash(pytest *)
Bash(mypy *)
Bash(ruff *)
Bash(python *)
Bash(python3 *)
Bash(pip *)
Bash(pip3 *)
Bash(poetry *)
Bash(pipenv *)
```

### Cargo/Rust Projects

**Trigger:** `Cargo.toml` detected.

```
# ALLOW
Bash(cargo build *)
Bash(cargo test *)
Bash(cargo run *)
Bash(cargo check *)
Bash(cargo clippy *)
Bash(cargo fmt *)

# DENY - steer toward cargo
Bash(rustc *)
```

---

## User Preference Permission Sets

### Q1: Git Commits

**Question:** "How do you want to handle git commits?"

**If user allows commits:**
```
Bash(git add *)
Bash(git commit *)
```

**If user wants prompts:** No additional allows (default ask behavior).

### Q2: Ad-hoc Script Execution

**Question:** "Do you want to allow ad-hoc script execution?"

**Tier 1 - Local scripts folder (arbitrary execution):**

Scripts in `claude-execution-allowed/` within project directory. With acceptEdits mode, Claude has write access by default. **This enables arbitrary code execution** - Claude can write and run any script.

```
Bash(bun run claude-execution-allowed/*)    # npm/bun
Bash(uv run claude-execution-allowed/*)     # Python
Bash(bash claude-execution-allowed/*)       # shell
```

**Tier 2 - Temp scripts folder (session-scoped):**

Scripts in `/tmp/claude-execution-allowed/<project>/`. Claude prompts once per session for write access to /tmp, then can create and run scripts freely. Slightly safer than Tier 1 since user explicitly grants session access.

```
Bash(bun /tmp/claude-execution-allowed/*)             # npm/bun
Bash(uv run /tmp/claude-execution-allowed/*)          # Python
Bash(bash /tmp/claude-execution-allowed/*)            # shell
```

**Tier 3 - Blessed scripts only:**

Only enumerate specific scripts from package.json/pyproject.toml. No ad-hoc script creation. Note: Claude can still edit these scripts if they exist in the project, so this is not a complete sandbox.

**Tier 4 - No script execution:**

No script allows beyond package manager commands. Most restrictive - Claude cannot run any project scripts directly.

**Future:** Deno would be ideal for sandboxed scripting (no permissions by default, granular `--allow-*` flags).

### Q3: Web Access

**Question:** "What level of web access do you want?"

**Tier 1 - Full access (least restrictive):**
```
# ALLOW
WebFetch
WebSearch
mcp__llms-fetch__fetch             # if llms-fetch-mcp installed
Bash(curl *)

# ASK - state-changing HTTP methods should prompt
Bash(curl * -X POST *)
Bash(curl * -X PUT *)
Bash(curl * -X DELETE *)
Bash(curl * -X PATCH *)
Bash(curl * --request POST *)
Bash(curl * --request PUT *)
Bash(curl * --request DELETE *)
Bash(curl * --request PATCH *)
Bash(curl * --data *)              # catches --data, --data-raw, --data-binary, --data-urlencode
Bash(curl * -d *)
Bash(curl * -F *)                  # form data (implies POST)
Bash(curl * --form *)
Bash(curl * -T *)                  # upload file (PUT)
Bash(curl * --upload-file *)
```

Note: `curl *` has no prompt injection mitigations.

**Tier 2 - WebFetch + WebSearch (recommended):**
```
WebFetch
WebSearch
```

WebFetch has built-in prompt injection protections. WebSearch is generally harmless.

**Tier 3 - Specific domains only:**

Example for a TypeScript web app project:
```
WebFetch(domain:tanstack.com)
WebFetch(domain:convex.dev)
WebFetch(domain:bun.sh)
WebFetch(domain:typescriptlang.org)
WebFetch(domain:developer.mozilla.org)
WebFetch(domain:nodejs.org)
WebFetch(domain:react.dev)
WebFetch(domain:tailwindcss.com)
WebFetch(domain:ui.shadcn.com)
WebFetch(domain:zod.dev)
WebFetch(domain:trpc.io)
WebFetch(domain:prisma.io)
WebSearch
```

**Tier 4 - No web access:** No WebFetch or WebSearch permissions.

---

## MCP Permissions

**Syntax:** `mcp__<server>__<tool>` for specific tool, `mcp__<server>__*` for all tools.

**Trigger:** MCP server detected in `.mcp.json`.

```
mcp__convex__*                 # all Convex tools
mcp__puppeteer__*              # all Puppeteer tools
mcp__llms-fetch__fetch         # single tool from llms-fetch
```

---

## Standard Questions

The best-practices skill should ask these questions (can't be determined from filesystem):

1. **Git commits?** → Adds git add/commit to allow
2. **Ad-hoc scripts?** → Tier 1-4
3. **Web access?** → Tier 1-4

---

## Experiment Results (Session 3 Amendment)

### Experiment: `Bash(cat)` Deny Pattern

**Question:** Can we deny `cat` (bare command) to prevent file creation via redirection while still allowing `cat file.txt`?

**Setup:**
- Allow: `Bash(cat:*)`
- Deny: `Bash(cat)` (no wildcard)

**Results:**

| Command | Result |
|---------|--------|
| `cat README.md` | ✓ Allowed (matches `cat:*` allow) |
| `echo "test" \| cat > file.txt` | ✓ Denied (matches bare `cat` deny) |
| `cat > file.txt << 'EOF'...EOF` | ✗ Generic prompt (heredoc too complex for parser) |

**Additional patterns tested (none worked for heredoc):**
- `Bash(cat >*)` - no effect
- `Bash(cat*<<*)` - no effect
- `Bash('EOF')` - no effect
- `Bash(cat )` (trailing space) - no effect

**Conclusion:** `Bash(cat)` deny catches piped redirects where the parser falls back to the bare command, but heredocs are too complex for the parser and fall through to generic prompts. Workaround: Add CLAUDE.md guidance to use Write tool instead of cat redirects/heredocs.

**Recommendation:** Add to deny list:
```
Bash(cat)                      # catches simple redirect bypasses
```

Add to CLAUDE.md:
> Use the Write tool for creating files. Avoid `cat` with redirects or heredocs.

---

### Experiment: Edit/Read Ask and Deny Permissions

**Question:** Can we use ask/deny permissions for Edit and Read tools to protect sensitive files?

**Setup:**
- Ask: `Edit(package.json)`, `Edit(**/package.json)`
- Deny: `Read(**/.env*)`

**Results:**

| Rule | Test | Result |
|------|------|--------|
| `Edit(package.json)` ask | Edit root package.json | ✓ Prompted (even in acceptEdits mode) |
| `Edit(**/package.json)` ask | Edit web/package.json | ✓ Prompted |
| `Read(**/.env*)` deny | Read .env.test | ✓ Denied |
| `Read(**/.env*)` deny | Read web/.env.test | ✓ Denied |

**Note:** Had a debugging issue where duplicate `deny` arrays in JSON caused rules to be ignored - only the last `deny` array is used.

**Conclusion:** Edit and Read permissions support glob patterns (`**` for recursive). Can use:
- `ask` for files that sometimes need editing (package.json, critical configs)
- `deny` for files that should never be accessed (.env, credentials)

Denying Read also blocks Edit (can't edit what you can't read), so only `Read` deny is needed for secrets.

**Recommendation:** Add to permission sets:
```
# ASK - sensitive project files
Edit(package.json)
Edit(**/package.json)

# DENY - secrets and credentials (Read deny also blocks Edit)
Read(**/.env*)
Read(**/*credentials*)
Read(**/*secret*)
```

---

### Experiment: Session-Scoped Ask Permissions

**Question:** If we use an ask rule for a folder and grant "allow all edits during this session", does it persist?

**Setup:**
- Created `claude-execution-allowed/` folder
- Added `Edit(claude-execution-allowed/**)` to ask list

**Results:**

| Action | Result |
|--------|--------|
| First write to folder | ✓ Prompted with "allow all edits during this session" option |
| User selects session option | ✓ File created |
| Second edit to same folder | ✗ Still prompts! |

**Conclusion:** The "allow all edits during this session" option for ask rules does NOT persist. Each edit still prompts.

This confirms the `/tmp/claude-execution-allowed/` pattern from Session 2 is still needed - it works because it relies on the **directory access** prompt (which does persist), not the ask rule prompt.

**Recommendation:** For session-scoped script execution:
- Use `/tmp/claude-execution-allowed/<project>/` pattern (prompts once for /tmp write access)
- Or use a project-local `claude-execution-allowed/` folder with allow rules (always allowed, no prompt)

---

## Non-Interactive Mode (-p flag)

No separate configuration needed. Good interactive config naturally works for non-interactive:

| Permission | Non-Interactive Behavior |
|------------|-------------------------|
| ALLOW | Executes normally |
| ASK/DEFAULT | Fails (no user) - this is desired for dangerous operations |
| DENY | Fails with message |

If `-p` tasks fail due to missing permissions, add specific gaps rather than broadening permissions.

---

## User Declines Permission Setup

If user declines ("No, I prefer to handle permissions myself"), write no permissions. This is valid - some users prefer maximum visibility or are learning Claude Code.
