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

**Pod lifecycle:**
- `start()` — spin up pod from config, auto-creates first kernel, returns kernel_id
- `stop()` or `terminate()` — dynamically named based on user's cleanup config
- `status()` — pod info, spend, uptime, and status of all kernels

**Execution:**
- `execute(kernel_id, code, timeout?)` — run Python in a kernel. Returns output if done in time, otherwise returns partial output + execution_id
- `get_output(kernel_id, execution_id, wait?, timeout?)` — check on / wait for a timed-out execution. execution_id is an incrementing integer per kernel. Mirrors Bash tool's TaskOutput pattern
- `create_kernel()` — spin up an additional kernel
- `interrupt(kernel_id)` — interrupt running execution
- `restart_kernel(kernel_id)` — restart kernel (fresh state)
- `shutdown_kernel(kernel_id)` — shut down a kernel and free resources

**Files:**
- `sync()` — rsync local code to pod (explicit, not automatic)
- `download(remote_path, local_path)` — pull file or folder back locally (folders tarred under the hood)

## Execution Model

- Jupyter kernel over WebSocket as the execution interface
- Claude writes normal Python — stateful kernel preserves variables, loaded models, etc.
- kernel_id always required (explicit about which kernel)
- Timeout follows Bash tool pattern: returns result if fast, returns execution_id if slow
- Execute while kernel is busy: returns error with clear options (get_output, interrupt, or use different kernel)
- Multiple kernels supported for parallelism

## Code & Data Flow

- Local repo is source of truth for code (git tracked, reviewable)
- Explicit sync via `sync()` tool (rsync over SSH, MCP server manages ephemeral keypair transparently)
- Respects .gitignore for sync
- Network volumes for large persistent data (datasets, checkpoints, weights)
- Network volume management is user-driven (optional part of setup, MCP does not create them)
- Results pulled back explicitly — Claude can inspect remote files via kernel, only copies back what it needs

## Observability

- All kernel activity auto-saved as .ipynb notebook files locally (one per kernel, new file on restart)
- Serves as audit trail for human review — researchers can open in Jupyter to see everything Claude did
- Claude works via tool responses during the session, notebook is for after-the-fact review
- User chooses in setup whether notebooks are committed to git or gitignored
- If committed: CLAUDE.md instruction to commit notebooks (v1, may upgrade to hook later)
- Claude can read the local .ipynb to recover context after compaction. Add a dedicated history tool only if this proves insufficient

## Docker Image

- MCP server only assumes Jupyter + SSH exist on the pod. All adaptation logic lives in the setup skill
- Default: `runpod/pytorch` (already has Jupyter, SSH, rsync, Python, uv)
- Custom images: setup skill checks what's missing and generates a startup script to install prerequisites
- Config stores image name + any custom startup commands; MCP server passes them through to RunPod API
- RunPod official images activate Jupyter via `JUPYTER_PASSWORD` env var, SSH via `PUBLIC_KEY` env var
- Watchdog injected at pod creation via startup command
- RunPod hook mechanism: `/post_start.sh` runs after services start (for runpod/base derivatives)

## Pod Lifecycle

- Claude spins up pods on demand via MCP tool
- On-demand instances only (no spot instances)
- GPU type / image / volume configured via config file (Claude can override)
- Setup skill walks users through configuration options, can improvise for non-standard setups
- v1: one pod per session. v2: multiple pods

## Cleanup

- MCP server cleans up pod on graceful shutdown (stop or terminate per user config)
- Inactivity watchdog on the pod: background process runs `runpodctl stop pod` if no heartbeat from MCP server for 5 minutes
- Stop hook (bundled in plugin): reads pod ID from state file + verifies via RunPod API. Blocks exit until pod is stopped/terminated per config

## Config & State

- Config and state live in `.claude/remote-kernels/` in the project directory
- Config written by setup skill (GPU type, image, startup commands, budget cap, cleanup behavior, notebook commit preference)
- State file (e.g. `state.json`) maintained by MCP server with current pod ID — read by Stop hook

## Authentication

- RunPod API key via environment variable `RUNPOD_API_KEY` (with .env support)
- Jupyter token set by MCP server at pod creation
- SSH keypair: ephemeral, generated per pod by MCP server (for rsync)
- Pod has auto-injected `RUNPOD_API_KEY` (pod-scoped) and `RUNPOD_POD_ID` (used by watchdog)

## Budget

- Configurable per-session budget cap (set in config by setup skill)
- MCP server tracks spend and warns Claude when approaching limit

## Platform

- RunPod (primary user base is MATS fellows who already use it)
- Jupyter comes built into RunPod templates
- RunPod proxy exposes HTTP/WS ports without SSH

## Key Findings from Docs

### RunPod API
- Two APIs: REST (`rest.runpod.io/v1`) and GraphQL (`api.runpod.io/graphql`). REST is newer but GraphQL has features REST doesn't (e.g. `stopAfter`)
- Pod creation: `POST /v1/pods` with `gpuTypeIds`, `imageName`, `networkVolumeId`, etc.
- Network volumes: persist across pod termination, shareable, S3-compatible access, must be in same datacenter as pod
- Every pod gets auto-injected `RUNPOD_API_KEY` (pod-scoped) and `RUNPOD_POD_ID` env vars
- `runpodctl` is pre-installed in all pods — can self-stop with `runpodctl stop pod $RUNPOD_POD_ID`
- GraphQL-only: `stopAfter` and `terminateAfter` fields (absolute timestamps) for auto-cleanup
- Default $80/hr spend cap (not configurable via API, contact support to change)
- `currentSpendPerHr` and `clientBalance` available via GraphQL `myself` query
- Storage costs: volume disk $0.20/GB/month stopped, network volume $0.07/GB/month
- `runpod/base` image includes: Ubuntu, CUDA, Python 3.9-3.13, uv, JupyterLab, openssh-server, nginx, rsync, git, tmux
- `runpod/pytorch` extends `runpod/base` with PyTorch + torchvision + torchaudio
- Services activated via env vars: `JUPYTER_PASSWORD` for Jupyter, `PUBLIC_KEY` for SSH
- `/post_start.sh` hook runs after services start (for custom setup in runpod/base derivatives)

### Jupyter Kernel API
- Kernel management is REST: `POST /api/kernels` to create, `DELETE` to kill, `POST .../restart` to restart
- Code execution requires WebSocket to `/api/kernels/{kernel_id}/channels`
- Send `execute_request`, receive stream of messages: `stream` (stdout/stderr), `execute_result`, `display_data` (plots as base64 PNG), `error`
- Kernels are persistent — variables, imports, loaded models survive between executions
- Multiple simultaneous kernels supported, each with independent state
- Auth via token in header: `Authorization: token <token>`
- Jupyter Contents API exists for file upload/download but is single-file, base64-encoded — not ideal for bulk sync

### Vast.ai (not using, but for reference)
- Has a Python SDK (`vastai-sdk`) and CLI (`vastai`)
- Has an `execute` API endpoint for running commands without SSH
- Volumes are tied to physical machines (not portable)
- No programmatic spending limits
