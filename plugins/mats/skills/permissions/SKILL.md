---
name: permissions
description: This skill should be used when the user asks to "set up permissions", "configure permissions", "fix permission prompts", "allow commands", "update permissions", "reduce prompts", "stop asking for permission", or mentions Claude Code permission configuration. Also use when permission-related friction is causing workflow issues.
---

# Claude Code Permissions Configuration

Generate permission configurations that enable autonomous Claude operation while maintaining security.

## Purpose

Proper permissions let Claude work without constant permission prompts while respecting user preferences. This is essential for:
- Running Claude asynchronously (without `--dangerously-skip-permissions`)
- Reducing friction in interactive sessions
- Preventing bypass vectors and steering toward blessed alternatives

## Key Principle: Ask vs Deny

**Deny** is for commands that are **never useful** - steering toward blessed alternatives.

**Ask** is safer than deny for **sometimes dangerous** commands. With ask, the session stops until the user provides attention. With deny, a potentially compromised model might try variations autonomously. Use ask for commands that are sometimes legitimate but could be misused.

## Workflow Overview

1. Detect project type (automatic)
2. Ask user preferences (5-6 questions)
3. Audit existing permissions (if any)
4. Generate settings with chunked edits
5. Add CLAUDE.md guidance section

---

## Step 1: Project Detection

Detect the project type automatically before asking questions. The goal is to:
- Identify the **blessed way** to run commands (package manager, language toolchain)
- **Enumerate specific scripts** from config files to allow individually
- Ensure commands needed **mid-session** (linting, testing, building) are allowed
- Deny non-blessed alternatives to steer Claude toward correct patterns

### What to Detect

**Package manager:** Check for lock files (bun.lockb, pnpm-lock.yaml, yarn.lock, package-lock.json, uv.lock, Cargo.lock). If ambiguous, ask the user.

**Scripts:** Extract script names from `package.json` scripts, `pyproject.toml` scripts, or similar config. These will be allowed individually - not all package manager calls. For monorepos, scripts may only exist in specific workspaces - identify the correct invocation pattern (e.g., `bun run --filter <workspace> <script>`).

**MCP servers:** Check `.mcp.json` for configured servers.

**hive-mind plugin:** If the `hive-mind:retrieval` subagent is available, include hive-mind commands in the universal safe commands (see Chunk 1 for the full list).

If it's unclear how to run commands in this project (e.g., Python project without pyproject.toml, unusual monorepo structure), ask the user about their preferred toolchain and the correct way to run scripts.

---

## Step 2: User Preference Questions

Before asking questions, give the user a brief overview of the main risks that permissions help manage:
- **Data loss:** Accidental deletion or overwriting of files/work
- **Information leakage:** Exposing private data to external services
- **System compromise:** Executing malicious or unintended code

Use the AskUserQuestion tool to gather preferences. Ask all questions together for efficiency.

### Q1: Git Commits

"How do you want to handle git commits?"

- **Allow commits (Recommended)** - Claude can stage and commit without a permission prompt. Especially useful when Claude is instructed to make small commits while working. Commits can always be reverted - this isn't push permission.
- **Prompt for commits** - Claude asks before each commit (default behavior). Choose this for more control over commit history.

### Q2: Ad-hoc Script Execution

"What level of ad-hoc script execution do you want to allow?"

Choose based on project sensitivity, potential for data loss, and execution environment (cloud environments are more forgiving).

- **No executable code** - Most restrictive. No project scripts, no testing, no ad-hoc scripts. Only linting and formatting work. For highly sensitive projects where even test execution could be a vector.
- **Blessed scripts only** - Only scripts enumerated in package.json/pyproject.toml. No ad-hoc script creation. Testing and project scripts work.
- **Temp scripts folder (Recommended)** - Scripts in `/tmp/claude-execution-allowed/<project>/` are allowed. The user gets one permission prompt per session when Claude first writes to this location, then ad-hoc scripts work without further permission prompts.
- **Local scripts folder** - Scripts in `claude-execution-allowed/` within the project are always allowed. Most permissive - enables arbitrary code execution without permission prompts.

### Q3: Web Access

"What level of web access do you want?"

- **Specific domains only** - Only documentation sites relevant to the project. Most restrictive.
- **WebFetch + WebSearch (Recommended)** - All domains via WebFetch and WebSearch. Almost always fine thanks to built-in prompt injection protections.
- **Full access with curl** - Adds curl for GET requests. Claude sees full file content and can download files, but adds some prompt injection risk since curl has no built-in mitigations.

### Q4: Default Mode

"Do you want to set plan mode as the default? (Recommended)"

- **Yes** - Claude researches and plans before making changes. Users often find plan mode useful in situations they didn't anticipate.
- **No** - Standard mode where Claude acts immediately.

### Q5: Settings File Strategy

"Where should permissions be stored?"

- **Split (Recommended)** - Universal safe commands and project detection go to settings.json (shared). User preferences (git, scripting, web) go to settings.local.json (personal). More conservative - collaborators start with safe defaults.
- **settings.json only** - All permissions in the shared file. Works across collaborators and environments, but shares your personal preferences.
- **settings.local.json only** - All permissions personal. For cases where collaborators have strong existing preferences you don't want to override.

### Q6: MCP Server Permissions (only if MCP servers detected)

For each MCP server found in `.mcp.json`, ask whether to allow all tools or leave on default (permission prompt per tool).

Use judgment based on:
- **Does it have many tools used heavily mid-session?** (e.g., playwright, puppeteer for testing) → Prefer allowing all
- **Does it access private data or modify external state, especially production?** (e.g., database, Airtable, email) → Prefer default ask

---

## Step 3: Audit Existing Permissions

After detection and preferences are known, read `.claude/settings.json` and `.claude/settings.local.json` if they exist.

### Check for Bypass Vectors

Commands that can execute arbitrary code should never be in allow lists:

```
Bash(env *)           # env VAR=val COMMAND runs any command
Bash(xargs *)         # pipes input to any command
Bash(bash *)          # executes arguments as bash
Bash(sh *)            # executes arguments as shell
Bash(bash -c *)       # executes string as bash
Bash(sh -c *)         # executes string as shell
Bash(eval *)          # evaluates arbitrary code
Bash(time *)          # time COMMAND runs any command
Bash(timeout *)       # timeout N COMMAND runs any command
Bash(exec *)          # replaces shell with command
Bash(nohup *)         # nohup COMMAND runs any command
Bash(nice *)          # nice COMMAND runs any command
Bash(python -c *)     # executes Python string
Bash(python3 -c *)    # executes Python string
Bash(node -e *)       # executes JavaScript string
Bash(perl -e *)       # executes Perl string
Bash(ruby -e *)       # executes Ruby string
Bash(bun run *)       # runs arbitrary scripts (allow specific scripts instead)
Bash(npm run *)       # runs arbitrary scripts (allow specific scripts instead)
```

The general pattern: any command that takes another command or code string as an argument is a bypass vector.

### Check for Deprecated Syntax

Look for rules using `:*` instead of ` *` (space-star). The colon syntax is deprecated.

When replacing, choose correctly:
- `cmd *` (space before star) - for commands that always need arguments
- `cmd*` (no space) - for commands that can run without arguments, or when you want to match `cmd && other`

### Check for Redundancy

- Duplicate rules
- Overly broad rules that subsume specific ones
- Overly specific rules that duplicate what the new permission sets will add
- Rules that conflict with the new configuration

Fix each issue individually - the user might have niche permissions they want to keep.

---

## Step 4: Generate Settings

Make edits in logical chunks with **separate Edit tool calls** for each chunk. This allows the user to review and approve each category independently. Provide brief reasoning before each chunk so the user understands what's being added.

**What to copy verbatim vs. adapt:**
- **Verbatim:** Chunk 1 (Universal Safe Commands), Chunk 3 (Git), Chunk 7 (Default Mode)
- **Mostly standard:** Chunk 5 (Web Access) - structure is standard, just pick the right option
- **Adapt paths/runtime:** Chunk 4 (Script Execution) - adapt project name, paths, and runtime (`uv run`, `bun`, `python3`, etc.)
- **Requires judgment:** Chunk 2 (Project-Specific Commands) - must match actual scripts and invocation patterns; Chunk 6 (Secret Protection) - must check what secret files actually exist in the project

### Chunk 1: Universal Safe Commands

Reasoning: "Adding read-only inspection commands and safe utilities..."

These are the universal safe commands. Note the wildcard patterns:
- `cmd*` for commands commonly used without arguments
- `cmd *` for commands that always need arguments

If hive-mind plugin is detected (see Step 1), also include these in the allow list:
- `Bash(hive-mind)` - no arguments, shows help
- `Bash(hive-mind read)` - shows read subcommand help
- `Bash(hive-mind read *)`
- `Bash(hive-mind search)` - shows search subcommand help
- `Bash(hive-mind search *)`

```json
{
  "permissions": {
    "allow": [
      "Bash(ls*)",
      "Bash(find *)",
      "Bash(cat *)",
      "Bash(head *)",
      "Bash(tail *)",
      "Bash(wc *)",
      "Bash(file *)",
      "Bash(stat *)",
      "Bash(du*)",
      "Bash(df*)",
      "Bash(diff *)",
      "Bash(tree*)",
      "Bash(realpath *)",
      "Bash(basename *)",
      "Bash(dirname *)",
      "Bash(mkdir *)",
      "Bash(grep *)",
      "Bash(rg *)",
      "Bash(awk *)",
      "Bash(sed -n *)",
      "Bash(jq *)",
      "Bash(yq *)",
      "Bash(sort *)",
      "Bash(uniq *)",
      "Bash(cut *)",
      "Bash(tr *)",
      "Bash(printf *)",
      "Bash(tee *)",
      "Bash(md5sum *)",
      "Bash(sha256sum *)",
      "Bash(base64 *)",
      "Bash(echo *)",
      "Bash(pwd)",
      "Bash(which *)",
      "Bash(type *)",
      "Bash(command -v *)",
      "Bash(uname*)",
      "Bash(whoami)",
      "Bash(date*)",
      "Bash(ps*)",
      "Bash(pgrep *)",
      "Bash(nvidia-smi*)",
      "Bash(printenv*)",
      "Bash(id)",
      "Bash(hostname)",
      "Bash(uptime)",
      "Bash(sleep *)",
      "Bash(export *)",
      "Bash(test *)",
      "Bash(man *)",
      "Bash(less *)",
      "Bash(readlink *)",
      "Bash(curl *://localhost*)",
      "Bash(curl *://127.0.0.1*)",
      "Bash(curl *://0.0.0.0*)",
      "Bash(git status*)",
      "Bash(git diff*)",
      "Bash(git log*)",
      "Bash(git show *)",
      "Bash(git branch)",
      "Bash(git remote)",
      "Bash(git remote -v)",
      "Bash(git stash list)",
      "Bash(git rev-parse *)",
      "Bash(git ls-files*)",
      "Bash(xargs cat*)",
      "Bash(xargs file*)",
      "Bash(xargs head*)",
      "Bash(xargs tail*)",
      "Bash(xargs wc*)",
      "Bash(xargs stat*)",
      "Bash(xargs sort*)",
      "Bash(xargs du*)",
      "Bash(xargs diff*)",
      "Bash(xargs basename*)",
      "Bash(xargs dirname*)",
      "Bash(xargs grep *)",
      "Bash(xargs cut *)",
      "Bash(xargs sed -n*)",
      "Bash(xargs -I{} cat *)",
      "Bash(xargs -I{} file *)",
      "Bash(xargs -I{} head *)",
      "Bash(xargs -I{} tail *)",
      "Bash(xargs -I{} wc *)",
      "Bash(xargs -I{} stat *)",
      "Bash(xargs -I{} sort *)",
      "Bash(xargs -I{} du *)",
      "Bash(xargs -I{} diff *)",
      "Bash(xargs -I{} basename *)",
      "Bash(xargs -I{} dirname *)",
      "Bash(xargs -I{} grep *)",
      "Bash(xargs -I{} cut *)",
      "Bash(xargs -I{} sed -n *)"
    ],
    "deny": [
      "Bash(for *)",
      "Bash(while *)",
      "Bash(until *)",
      "Bash(timeout *)",
      "Bash(env *)",
      "Bash(find * -exec *)",
      "Bash(find * -execdir *)",
      "Bash(awk *system\\(*)",
      "Bash(xargs awk*system\\(*)",
      "Bash(xargs -I{} awk*system\\(*)",
      "Bash(xargs sh *)",
      "Bash(xargs -I{} sh *)",
      "Bash(xargs bash *)",
      "Bash(xargs -I{} bash *)",
      "Bash(cat)"
    ]
  }
}
```

### Chunk 2: Project-Specific Commands

Based on detected package manager, add commands for enumerated scripts. These are **examples** - you must adapt to the actual project structure, scripts, and invocation patterns.

**Example for bun project with scripts: dev, build, test, lint, format, typecheck:**

Reasoning: "Adding your project's package.json scripts via bun..."

**Pattern note:** For each script, use two patterns: exact match (`bun run dev`) and with-arguments (`bun run dev *`). This allows passing flags like `--port 3000` without accidentally matching other scripts (e.g., `bun run dev*` would also match `bun run devscript.js`).

```json
{
  "permissions": {
    "allow": [
      "Bash(bun run dev)",
      "Bash(bun run dev *)",
      "Bash(bun run build)",
      "Bash(bun run build *)",
      "Bash(bun run test)",
      "Bash(bun run test *)",
      "Bash(bun run lint)",
      "Bash(bun run lint *)",
      "Bash(bun run format)",
      "Bash(bun run format *)",
      "Bash(bun run typecheck)",
      "Bash(bun run typecheck *)",
      "Bash(bun --version)"
    ],
    "deny": [
      "Bash(eslint *)",
      "Bash(prettier *)",
      "Bash(tsc *)",
      "Bash(jest *)",
      "Bash(vitest *)",
      "Bash(node *)",
      "Bash(npx *)",
      "Bash(npm *)",
      "Bash(pnpm *)",
      "Bash(yarn *)"
    ]
  }
}
```

Note: `bun install`, `bun add`, `bun remove` are left on default ask - rarely needed mid-session and users often want control.

**Example for uv/Python project with scripts in scripts/ directory:**

```json
{
  "permissions": {
    "allow": [
      "Bash(uv run pytest)",
      "Bash(uv run pytest *)",
      "Bash(uv run mypy)",
      "Bash(uv run mypy *)",
      "Bash(uv run ruff)",
      "Bash(uv run ruff *)",
      "Bash(uv run scripts/train.py)",
      "Bash(uv run scripts/train.py *)",
      "Bash(uv run scripts/eval.py)",
      "Bash(uv run scripts/eval.py *)",
      "Bash(uv run scripts/analyze.py)",
      "Bash(uv run scripts/analyze.py *)",
      "Bash(uv --version)"
    ],
    "deny": [
      "Bash(pytest *)",
      "Bash(mypy *)",
      "Bash(ruff *)",
      "Bash(python *)",
      "Bash(python3 *)",
      "Bash(pip *)",
      "Bash(pip3 *)",
      "Bash(poetry *)",
      "Bash(pipenv *)"
    ]
  }
}
```

Note: `uv sync`, `uv add`, `uv remove` are left on default ask.

**Example for Cargo/Rust project:**

```json
{
  "permissions": {
    "allow": [
      "Bash(cargo build)",
      "Bash(cargo build *)",
      "Bash(cargo test)",
      "Bash(cargo test *)",
      "Bash(cargo run)",
      "Bash(cargo run *)",
      "Bash(cargo check)",
      "Bash(cargo check *)",
      "Bash(cargo clippy)",
      "Bash(cargo clippy *)",
      "Bash(cargo fmt)",
      "Bash(cargo fmt *)",
      "Bash(cargo --version)"
    ],
    "deny": [
      "Bash(rustc *)"
    ]
  }
}
```

Note: `cargo add`, `cargo remove` are left on default ask.

### Chunk 3: Git Permissions (if user allows commits)

Reasoning: "Adding git add and commit permissions as requested..."

```json
{
  "permissions": {
    "allow": [
      "Bash(git add *)",
      "Bash(git commit *)"
    ]
  }
}
```

### Chunk 4: Script Execution (based on preference)

**Local scripts folder:**

Reasoning: "Adding local script execution in claude-execution-allowed/..."

Adapt to the project's scripting language. For Python projects using uv:

```json
{
  "permissions": {
    "allow": [
      "Bash(uv run claude-execution-allowed/*)",
      "Bash(bash claude-execution-allowed/*)",
      "Bash(chmod +x claude-execution-allowed/*)",
      "Bash(xargs uv run claude-execution-allowed/*)",
      "Bash(xargs bash claude-execution-allowed/*)",
      "Bash(xargs -I{} uv run claude-execution-allowed/*)",
      "Bash(xargs -I{} bash claude-execution-allowed/*)"
    ]
  }
}
```

For JavaScript/TypeScript projects that use bun:

```json
{
  "permissions": {
    "allow": [
      "Bash(bun claude-execution-allowed/*)",
      "Bash(bash claude-execution-allowed/*)",
      "Bash(chmod +x claude-execution-allowed/*)",
      "Bash(xargs bun claude-execution-allowed/*)",
      "Bash(xargs bash claude-execution-allowed/*)",
      "Bash(xargs -I{} bun claude-execution-allowed/*)",
      "Bash(xargs -I{} bash claude-execution-allowed/*)"
    ]
  }
}
```

If unclear what the correct way to run scripts is, ask the user.

**Temp scripts folder:**

Reasoning: "Adding temp script execution in /tmp/claude-execution-allowed/[project-name]/..."

Use the project name in the path. Example for a Python project named "my-research":

```json
{
  "permissions": {
    "allow": [
      "Bash(uv run /tmp/claude-execution-allowed/my-research/*)",
      "Bash(bash /tmp/claude-execution-allowed/my-research/*)",
      "Bash(chmod +x /tmp/claude-execution-allowed/my-research/*)",
      "Bash(xargs uv run /tmp/claude-execution-allowed/my-research/*)",
      "Bash(xargs bash /tmp/claude-execution-allowed/my-research/*)",
      "Bash(xargs -I{} uv run /tmp/claude-execution-allowed/my-research/*)",
      "Bash(xargs -I{} bash /tmp/claude-execution-allowed/my-research/*)"
    ]
  }
}
```

**Blessed scripts only:** No additional script permissions beyond enumerated project scripts.

### Chunk 5: Web Access (based on preference)

**Full access with curl:**

Reasoning: "Adding full web access including curl for GET requests..."

```json
{
  "permissions": {
    "allow": [
      "WebFetch",
      "WebSearch",
      "Bash(curl *)"
    ],
    "ask": [
      "Bash(curl* -X POST*)",
      "Bash(curl* -X PUT*)",
      "Bash(curl* -X DELETE*)",
      "Bash(curl* -X PATCH*)",
      "Bash(curl* --request POST*)",
      "Bash(curl* --request PUT*)",
      "Bash(curl* --request DELETE*)",
      "Bash(curl* --request PATCH*)",
      "Bash(curl* --data*)",
      "Bash(curl* -d *)",
      "Bash(curl* -F *)",
      "Bash(curl* --form*)",
      "Bash(curl* -T *)",
      "Bash(curl* --upload-file*)"
    ]
  }
}
```

**WebFetch + WebSearch:**

Reasoning: "Adding WebFetch and WebSearch for all domains..."

```json
{
  "permissions": {
    "allow": [
      "WebFetch",
      "WebSearch"
    ]
  }
}
```

**Specific domains:**

Generate 10+ relevant documentation domains based on project type. Choose official documentation sites.

Example for a TypeScript/React project:

```json
{
  "permissions": {
    "allow": [
      "WebFetch(domain:react.dev)",
      "WebFetch(domain:typescriptlang.org)",
      "WebFetch(domain:developer.mozilla.org)",
      "WebFetch(domain:nodejs.org)",
      "WebFetch(domain:bun.sh)",
      "WebFetch(domain:vitejs.dev)",
      "WebFetch(domain:tailwindcss.com)",
      "WebFetch(domain:ui.shadcn.com)",
      "WebFetch(domain:tanstack.com)",
      "WebFetch(domain:zod.dev)",
      "WebFetch(domain:trpc.io)",
      "WebFetch(domain:nextjs.org)",
      "WebSearch"
    ]
  }
}
```

Example for a Python ML project:

```json
{
  "permissions": {
    "allow": [
      "WebFetch(domain:docs.python.org)",
      "WebFetch(domain:pytorch.org)",
      "WebFetch(domain:numpy.org)",
      "WebFetch(domain:pandas.pydata.org)",
      "WebFetch(domain:scikit-learn.org)",
      "WebFetch(domain:huggingface.co)",
      "WebFetch(domain:wandb.ai)",
      "WebFetch(domain:docs.ray.io)",
      "WebFetch(domain:jax.readthedocs.io)",
      "WebFetch(domain:einops.rocks)",
      "WebSearch"
    ]
  }
}
```

### Chunk 6: Secret Protection

Reasoning: "Adding protection for sensitive files..."

**Important:** Actually check what secret files exist in the project - don't use a generic list. Look for `.env*` files, credentials, keys, etc. Only block files that actually exist or are likely to be created.

Common patterns (adapt to what you find):

```json
{
  "permissions": {
    "deny": [
      "Read(**/.env)",
      "Read(**/.env.local)",
      "Read(**/.envrc)"
    ]
  }
}
```

Note: `.env.example` and `.env.test` are typically safe to read - don't block those unless they contain real secrets.

For projects with credentials files (if present):
```json
{
  "permissions": {
    "deny": [
      "Read(**/.aws/credentials)",
      "Read(**/.ssh/*)",
      "Read(**/*.pem)",
      "Read(**/*_rsa)",
      "Read(**/*_ed25519)"
    ]
  }
}
```

### Chunk 7: Default Mode (if user chose plan mode)

Reasoning: "Setting plan mode as the default..."

Note: `defaultMode` must be inside the `permissions` object.

```json
{
  "permissions": {
    "defaultMode": "plan"
  }
}
```

### Chunk 8: MCP Permissions (if user approved specific servers)

For servers the user approved for full access:

```json
{
  "permissions": {
    "allow": [
      "mcp__playwright__*",
      "mcp__puppeteer__*"
    ]
  }
}
```

Servers left on default ask don't need any configuration.

---

## Step 5: Add CLAUDE.md Section

Add a section to CLAUDE.md with command execution guidance. Use a heading that fits the project - "## Running Commands", "## Scripts and Commands", or similar. Content varies by script execution preference and project type.

### For Local scripts folder (claude-execution-allowed/)

```markdown
## Running Commands

Run scripts via `[package-manager] run <script>`. Available scripts: [list enumerated scripts].

**Ad-hoc scripts:** Only the `claude-execution-allowed/` directory is approved for ad-hoc scripts. Write scripts there and run with `[uv run/bun/bash] claude-execution-allowed/<script-name>`. Scripts can be [Python/JavaScript] or bash depending on the task. For bash scripts, make them executable first with `chmod +x`.

**Bash operations:**

Complex bash syntax is hard for Claude Code to permission correctly. Keep commands simple.

Simple operations are fine: `|`, `||`, `&&`, `>` redirects.

For bulk operations on multiple files, use xargs:
- Plain: `ls *.md | xargs wc -l`
- With placeholder: `ls *.md | xargs -I{} head -1 {}`

For string interpolation (`$()`, backticks, `${}`), heredocs, loops, or advanced xargs flags (`-P`, `-L`, `-n`), write a script in `claude-execution-allowed/` instead.

**Patterns:**
- File creation: Write tool, not `cat << 'EOF' > file`
- Env vars: `export VAR=val && command`, not `VAR=val command` or `env VAR=val command`
- Bulk operations: `ls *.md | xargs wc -l`, not `for f in *.md; do cmd "$f"; done`
- Parallel/batched xargs: script, not `xargs -P4` or `xargs -L1`
- Per-item shell logic: script, not `xargs sh -c '...'`

If a command that should be allowed is denied, or if project structure changes significantly, ask about running `/mats:permissions` to update settings.
```

### For Temp scripts folder (/tmp/claude-execution-allowed/)

```markdown
## Running Commands

Run scripts via `[package-manager] run <script>`. Available scripts: [list enumerated scripts].

**Ad-hoc scripts:** Only `/tmp/claude-execution-allowed/[project-name]/` is approved for ad-hoc scripts. Write scripts there and run with `[uv run/bun/bash] /tmp/claude-execution-allowed/[project-name]/<script-name>`. Scripts can be [Python/JavaScript] or bash depending on the task. For bash scripts, make them executable first with `chmod +x`.

**Bash operations:**

Complex bash syntax is hard for Claude Code to permission correctly. Keep commands simple.

Simple operations are fine: `|`, `||`, `&&`, `>` redirects.

For bulk operations on multiple files, use xargs:
- Plain: `ls *.md | xargs wc -l`
- With placeholder: `ls *.md | xargs -I{} head -1 {}`

For string interpolation (`$()`, backticks, `${}`), heredocs, loops, or advanced xargs flags, write a script in `/tmp/claude-execution-allowed/[project-name]/` instead.

**Patterns:**
- File creation: Write tool, not `cat << 'EOF' > file`
- Env vars: `export VAR=val && command`, not `VAR=val command` or `env VAR=val command`
- Bulk operations: `ls *.md | xargs wc -l`, not `for f in *.md; do cmd "$f"; done`
- Parallel/batched xargs: script, not `xargs -P4` or `xargs -L1`
- Per-item shell logic: script, not `xargs sh -c '...'`

If a command that should be allowed is denied, or if project structure changes significantly, ask about running `/mats:permissions` to update settings.
```

### For No executable code (most restrictive)

For this tier, don't include script-related content. Only include the basic bash operations section:

```markdown
## Running Commands

**Bash operations:**

Complex bash syntax is hard for Claude Code to permission correctly. Keep commands simple.

Simple operations are fine: `|`, `||`, `&&`, `>` redirects.

For bulk operations on multiple files, use xargs:
- Plain: `ls *.md | xargs wc -l`
- With placeholder: `ls *.md | xargs -I{} head -1 {}`

Avoid string interpolation (`$()`, backticks, `${}`), heredocs, loops, and advanced xargs flags - break into simpler sequential commands or ask the user for help.

**Patterns:**
- File creation: Write tool, not `cat << 'EOF' > file`
- Env vars: `export VAR=val && command`, not `VAR=val command` or `env VAR=val command`
- Bulk operations: `ls *.md | xargs wc -l`, not `for f in *.md; do cmd "$f"; done`

If a command that should be allowed is denied, or if project structure changes significantly, ask about running `/mats:permissions` to update settings.
```

### For Blessed scripts only

```markdown
## Running Commands

Run scripts via `[package-manager] run <script>`. Available scripts: [list enumerated scripts].

**Bash operations:**

Complex bash syntax is hard for Claude Code to permission correctly. Keep commands simple.

Simple operations are fine: `|`, `||`, `&&`, `>` redirects.

For bulk operations on multiple files, use xargs:
- Plain: `ls *.md | xargs wc -l`
- With placeholder: `ls *.md | xargs -I{} head -1 {}`

Avoid string interpolation (`$()`, backticks, `${}`), heredocs, loops, and advanced xargs flags - break into simpler sequential commands or ask the user for help.

**Patterns:**
- File creation: Write tool, not `cat << 'EOF' > file`
- Env vars: `export VAR=val && command`, not `VAR=val command` or `env VAR=val command`
- Bulk operations: `ls *.md | xargs wc -l`, not `for f in *.md; do cmd "$f"; done`

If a command that should be allowed is denied, or if project structure changes significantly, ask about running `/mats:permissions` to update settings.
```
