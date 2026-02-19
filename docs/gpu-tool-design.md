# remote-kernels — Design Doc

MCP server + plugin that lets Claude spin up cloud GPU instances and interact with persistent Jupyter kernels.

## Architecture

- Rust MCP server (fast startup, single binary, good WebSocket support)
- Distributed via plugin as platform-specific binaries on GitHub releases
- Bootstrap shell script as the MCP server command: checks platform, downloads binary if needed, runs it
- Tracks plugin version vs cached binary version, re-downloads on plugin update
- Note: verify this approach works with the Agent SDK (for GitHub Action integration)
- An official RunPod MCP exists (github.com/runpod/runpod-mcp) but is just CRUD for infrastructure — we're building the execution layer

## Plugin Components

- MCP server (Rust binary): pod management, kernel execution, file sync
- Stop hook: prevents Claude from exiting while a pod is running
- Setup skill: walks users through configuration, adapts to their environment
- CLAUDE.md additions: instructions for committing notebooks (if user opts in)

## MCP Tools

No prefix on tool names — MCP server name (`remote-kernels`) provides namespacing.

All tool names, descriptions, and parameter descriptions should live in a dedicated file, imported by the implementation. This makes them easy to review and edit in one place.

**Pod lifecycle:**
- `start(pod_id?)` — spin up pod from config, or connect to an existing pod by ID (does not create a kernel — use `create_kernel()`)
- `stop()` — stop the pod (preserves data, storage costs apply)
- `terminate()` — delete the pod (all non-network-volume data lost)
- `status()` — pod info, spend, uptime, and status of all kernels

**Execution:**
- `execute(kernel_id, code, timeout?, queue?)` — run Python in a kernel. Returns output if done in time, otherwise returns partial output + execution_id. If kernel is busy: error by default, or queue behind current execution if `queue=true`
- `get_output(kernel_id, execution_id, wait?, timeout?)` — check on / wait for a timed-out execution. execution_id is an incrementing integer per kernel. Mirrors Bash tool's TaskOutput pattern
- `create_kernel(name)` — spin up a named kernel. Name flows through to the notebook filename
- `interrupt(kernel_id)` — interrupt running execution
- `restart_kernel(kernel_id)` — restart kernel (fresh state)
- `shutdown_kernel(kernel_id)` — shut down a kernel and free resources

**Files:**
- `sync(include?)` — rsync local code to pod (explicit, not automatic). Respects `.gitignore`, but `include` parameter adds extra paths. Config also has a persistent `sync-include` list
- `download(remote_path, local_path)` — pull file or folder back locally (folders tarred under the hood)

## Execution Model

- Jupyter kernels are exclusively for Claude's code execution — no system/infrastructure use
- Jupyter kernel over WebSocket as the execution interface
- Claude writes normal Python — stateful kernel preserves variables, loaded models, etc.
- kernel_id always required (explicit about which kernel)
- Timeout follows Bash tool pattern: returns result if fast, returns execution_id if slow
- Execute while kernel is busy: error by default, or queue with `queue=true`
- Multiple kernels supported for parallelism

## Code & Data Flow

- Local repo is source of truth for code (git tracked, reviewable)
- Explicit sync via `sync()` tool (rsync over SSH, MCP server manages ephemeral keypair transparently)
- Respects .gitignore for sync
- Network volumes for large persistent data (datasets, checkpoints, weights)
- Network volume management is user-driven (optional part of setup, MCP does not create them)
- Results pulled back explicitly — Claude can inspect remote files via kernel, only copies back what it needs
- Default cleanup is terminate — pods should be treated as ephemeral
- RunPod network volumes have no snapshots or backups. Setup skill educates users about this and recommends external backup (HuggingFace Hub, W&B Artifacts, S3/Backblaze)

## Observability

- All kernel activity auto-saved as .ipynb notebook files locally (one per kernel, new file on restart)
- Notebook directory configurable, defaults to `remote-kernels/` at project root
- Serves as audit trail for human review — researchers can open in Jupyter to see everything Claude did
- Claude works via tool responses during the session, notebook is for after-the-fact review
- User chooses in setup whether notebooks are committed to git or gitignored
- If committed: CLAUDE.md instruction to commit notebooks (v1, may upgrade to hook later)
- Claude can read the local .ipynb to recover context after compaction. Add a dedicated history tool only if this proves insufficient

## Docker Image

- MCP server only assumes Jupyter + SSH exist on the pod. All adaptation logic lives in the setup skill
- Default: `runpod/pytorch` (has Jupyter, SSH, Python, uv — but not rsync, which must be installed via startup command)
- Custom images: setup skill checks what's missing and generates a startup script to install prerequisites
- Config stores image name + any custom startup commands; MCP server passes them through to RunPod API
- RunPod official images activate Jupyter via `JUPYTER_PASSWORD` env var, SSH via `PUBLIC_KEY` env var
- Watchdog and budget enforcement scripts injected at pod creation via startup command
- RunPod hook mechanism: `/post_start.sh` runs after services start (for runpod/base derivatives)

## Pod Lifecycle

- Claude spins up pods on demand via MCP tool
- Both on-demand and spot instances supported (setup skill guides the choice)
- Public IP required — `start()` fails with a clear error if the assigned machine has no public IP (needed for SSH heartbeat and file sync)
- GPU type / image / volume configured via config file (Claude can override)
- Setup skill dynamically helps users configure based on their environment, warns about inconsistent options
- v1: one pod per session. v2: multiple pods

### Pod Creation Retries

- `gpu-type-ids` is an ordered preference list — try each in sequence
- On 500 error: parse the error message to distinguish availability errors from unknown server errors
- Recognized availability error (e.g. "no instances available") → skip to next GPU type immediately
- Other 500 errors → retry up to 3 times with 1s delay, then move to next GPU type
- If all GPU types exhausted: return a clear error to Claude explaining which types were tried and why each failed, so Claude can decide what to try next

### Connecting to Existing Pods

- `start(pod_id)` connects to a pod not created by this session
- Sets up a new watchdog/heartbeat via SSH (two watchdogs is better than none)
- Budget enforcement script also injected via SSH
- Discovers existing kernels on the pod

## Cleanup

- MCP server cleans up pod on graceful shutdown (stop or terminate per user config, or disabled entirely)
- Watchdog injected via pod startup command — background process runs `runpodctl stop pod` (or `remove pod` per cleanup config) if no heartbeat for 5 minutes
- Heartbeat sent by MCP server over SSH — runs in background, does not block `start()`
- When cleanup is disabled: tool output on long-running executions reminds Claude to queue a cleanup command after the current cell completes
- Stop hook (bundled in plugin): reads pod ID from state file + verifies via RunPod API. Blocks exit until pod is stopped/terminated per config (skipped when cleanup is disabled)

## Config & State

- Config file (`remote-kernels.toml` at project root), split into two sections:
  - Top-level: fields the MCP server acts on (GPU preference list, cleanup behavior, notebook directory, sync-include, etc.)
  - `[runpod]` section: passed through transparently to the RunPod pod creation API (disk sizes, cloud type, network volume, etc.). Users can set any RunPod API option here without us needing to explicitly support it
- Claude can freely edit the config file (e.g. to fall back to a different GPU type)
- `start()` also accepts `gpu_type` and `image` overrides for one-off changes
- State file (`.claude/remote-kernels/state.json`) maintained by MCP server with current pod ID and accumulated spend — read by Stop hook
- `.claude/remote-kernels/` auto-creates a `.gitignore` with `*`

## Pod Environment Variables

Three ways to inject env vars into the pod (all optional, later sources override earlier):
- `inherit-env` — list of variable names to forward from the local environment / `.env` files to the pod
- `env-file` — path to an env file to load onto the pod (e.g. `.env.pod`)
- `[env]` section — explicit key-value pairs in the config (for non-secret values)

Setup skill recommends `inherit-env` by default, inspecting the project for relevant services (HuggingFace, W&B, etc.)

## Authentication

- RunPod API key via environment variable `RUNPOD_API_KEY` (with .env support)
- Jupyter token set by MCP server at pod creation
- SSH keypair: ephemeral, generated per pod by MCP server (for rsync and heartbeat)
- Pod has auto-injected `RUNPOD_API_KEY` (pod-scoped) and `RUNPOD_POD_ID` (used by watchdog)

## Budget

- Per-session budget cap via `REMOTE_KERNELS_BUDGET` environment variable (configured in `.claude/settings.json` — Claude Code's built-in guardrails prevent Claude from editing this). Can also be set in config file, but env var takes precedence
- MCP server calculates spend as `hourly_rate * uptime`, accumulated across stop/start cycles in state file
- Every `execute()` response includes current spend and remaining budget as a natural reminder
- Enforcement: pod-side script receives max runtime, runs cleanup action (stop or terminate per config) when time is up. Script injected via startup command for new pods, via SSH for `start(pod_id)`
- After budget exceeded: all MCP tools return an error explaining that the session budget was reached

## Platform

- RunPod (primary user base is MATS fellows who already use it)
- Jupyter comes built into RunPod templates
- RunPod proxy exposes HTTP/WS ports without SSH

## Security Limitations

- RunPod has no egress filtering, firewall rules, or network policies — pods have unrestricted outbound internet access
- No protection against data exfiltration via prompt injection at the platform level
- RunPod explicitly describes itself as "built for trusted team environments" with no sandboxing
- Network volumes cannot be mounted read-only — no platform-level protection against accidental writes

## Remaining Work

### Phase 1: Align existing implementation with design

- **Heartbeat → SSH**: move from dedicated Jupyter kernel to SSH, run in background (don't block `start()`)
- **rsync install**: move from Jupyter kernel to startup command
- **start() no auto-kernel**: remove auto-create, let Claude use `create_kernel(name)`
- **Kernel naming**: `create_kernel()` needs a `name` parameter, flows to notebook filename
- **Interrupt tool**: wire existing `interrupt_kernel()` as an MCP tool
- **Config split**: move RunPod-specific fields into `[runpod]` passthrough section
- **Retry logic**: parse 500 error messages, cycle through GPU types independently
- **Tool descriptions file**: extract all tool/server descriptions to a dedicated file

### Phase 2: New MCP server features

These can be done in any order:

- **Budget enforcement**: pod-side script for hard cutoff, spend tracking in state file, `REMOTE_KERNELS_BUDGET` env var, spend/remaining in `execute()` responses
- **Execution queuing**: `queue` parameter on `execute()`
- **get_output / execution_id**: poll mechanism for long-running executions
- **Connect to existing pod**: `start(pod_id)` with watchdog/heartbeat/budget setup via SSH
- **Cleanup disable option**: support disabling automatic cleanup, with tool reminders
- **Sync includes**: `include` parameter on `sync()` + `sync-include` config field
- **Pod env vars**: `inherit-env`, `env-file`, `[env]` config options

### Phase 3: Plugin and distribution

Depends on the MCP server being stable:

- **Plugin shell**: plugin.json, .mcp.json, setup skill, stop hook
- **Binary distribution**: GitHub releases, bootstrap script, platform-specific binaries

## Key Findings from Docs

### RunPod API
- Two APIs: REST (`rest.runpod.io/v1`) and GraphQL (`api.runpod.io/graphql`). REST is newer but GraphQL has features REST doesn't (e.g. `stopAfter`)
- Pod creation: `POST /v1/pods` with `gpuTypeIds`, `imageName`, `networkVolumeId`, etc.
- Network volumes: persist across pod termination, shareable, S3-compatible access, must be in same datacenter as pod
- Every pod gets auto-injected `RUNPOD_API_KEY` (pod-scoped) and `RUNPOD_POD_ID` env vars
- `runpodctl` is pre-installed in all pods — can self-stop with `runpodctl stop pod $RUNPOD_POD_ID`
- GraphQL-only: `stopAfter` and `terminateAfter` fields (absolute timestamps) for auto-cleanup
- Default $80/hr spend cap (not configurable via API, contact support to change)
- `currentSpendPerHr` and `clientBalance` available via GraphQL `myself` query (account-level, not per-pod)
- Storage costs: volume disk $0.20/GB/month stopped, network volume $0.07/GB/month
- `runpod/base` image includes: Ubuntu, CUDA, Python 3.9-3.13, uv, JupyterLab, openssh-server, nginx, rsync, git, tmux
- `runpod/pytorch` extends `runpod/base` with PyTorch + torchvision + torchaudio (but does NOT include rsync despite docs suggesting otherwise)
- Services activated via env vars: `JUPYTER_PASSWORD` for Jupyter, `PUBLIC_KEY` for SSH
- `/post_start.sh` hook runs after services start (for runpod/base derivatives)
- HTTP 500 returned for transient "no availability" errors — must parse error message string to distinguish from real server errors

### Jupyter Kernel API
- Kernel management is REST: `POST /api/kernels` to create, `DELETE` to kill, `POST .../restart` to restart
- Code execution requires WebSocket to `/api/kernels/{kernel_id}/channels`
- Send `execute_request`, receive stream of messages: `stream` (stdout/stderr), `execute_result`, `display_data` (plots as base64 PNG), `error`
- Kernels are persistent — variables, imports, loaded models survive between executions
- Multiple simultaneous kernels supported, each with independent state
- Auth via token in header: `Authorization: token <token>`
- Jupyter Contents API exists for file upload/download but is single-file, base64-encoded — not ideal for bulk sync
