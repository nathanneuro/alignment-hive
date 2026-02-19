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

Tool descriptions live as doc comments on each tool method in `server.rs` (rmcp proc macro requires string literals, can't reference a separate constants file).

**Pod lifecycle:**
- `start(gpu_type?, image?)` — spin up pod from config, with optional overrides
- `stop()` — stop the pod (preserves data, storage costs apply)
- `terminate()` — delete the pod (all non-network-volume data lost)
- `status()` — pod info, spend, uptime, cleanup mode, budget, and kernel list

**Execution:**
- `execute(kernel_id, code, timeout?, queue?)` — run Python in a kernel. Returns output if done in time, otherwise returns cell_number for `get_output()`. `timeout=0` for fire-and-forget. `queue=true` to wait behind a busy kernel instead of erroring.
- `get_output(kernel_id, cell_number, wait?, timeout?)` — check on / wait for a timed-out or fire-and-forget execution. cell_number matches the notebook cell index.
- `create_kernel(name?)` — spin up a named kernel. Name flows through to the notebook filename
- `interrupt(kernel_id)` — interrupt running execution
- `restart_kernel(kernel_id)` — restart kernel (fresh state)
- `shutdown_kernel(kernel_id)` — shut down a kernel and free resources

**Files:**
- `sync(include?)` — rsync local code to pod (explicit, not automatic). Respects `.gitignore`, but `include` parameter adds extra paths (validated: no `..` or absolute paths). Config also has a persistent `sync-include` list
- `download(remote_path, local_path)` — pull file or folder back locally

## Execution Model

- Jupyter kernels are exclusively for Claude's code execution — no system/infrastructure use
- Jupyter kernel over WebSocket as the execution interface
- Claude writes normal Python — stateful kernel preserves variables, loaded models, etc.
- kernel_id always required (explicit about which kernel)
- Timeout follows Bash tool pattern: returns result if fast, returns cell_number if slow
- `timeout=0`: fire-and-forget — starts execution, returns cell_number immediately
- Execute while kernel is busy: error by default, or queue with `queue=true`
- Queuing via select guard in WS loop — mpsc channel naturally buffers requests
- Multiple kernels supported for parallelism

## Code & Data Flow

- Local repo is source of truth for code (git tracked, reviewable)
- Explicit sync via `sync()` tool (rsync over SSH, MCP server manages ephemeral keypair transparently)
- Respects .gitignore for sync; `sync-include` config and `include` parameter for extra paths
- Network volumes for large persistent data (datasets, checkpoints, weights)
- Network volume management is user-driven (optional part of setup, MCP does not create them)
- Results pulled back explicitly — Claude can inspect remote files via kernel, only copies back what it needs
- Default cleanup is `terminate` — pods should be treated as ephemeral
- RunPod network volumes have no snapshots or backups. Setup skill educates users about this and recommends external backup (HuggingFace Hub, W&B Artifacts, S3/Backblaze)

## Observability

- All kernel activity auto-saved as .ipynb notebook files locally (one per kernel, new file on restart)
- Notebook directory configurable via `notebook-dir`, defaults to `remote-kernels/` at project root
- Not gitignored by default — separate from `.claude/remote-kernels/` internal state. Setup skill lets users choose whether to commit or gitignore.
- Serves as audit trail for human review — researchers can open in Jupyter to see everything Claude did
- Claude works via tool responses during the session, notebook is for after-the-fact review
- If committed: CLAUDE.md instruction to commit notebooks (v1, may upgrade to hook later)
- Claude can read the local .ipynb to recover context after compaction. Add a dedicated history tool only if this proves insufficient

## Docker Image

- MCP server only assumes Jupyter + SSH exist on the pod. All adaptation logic lives in the setup skill
- Default: `runpod/pytorch` (has Jupyter, SSH, Python, uv — but not rsync, which must be installed via startup command)
- Custom images: setup skill checks what's missing and generates a startup script to install prerequisites
- Config stores image name + any custom startup commands; MCP server passes them through to RunPod API
- RunPod official images activate Jupyter via `JUPYTER_PASSWORD` env var, SSH via `PUBLIC_KEY` env var
- Watchdog and budget enforcement scripts injected via SSH after pod starts
- RunPod hook mechanism: `/post_start.sh` runs after services start (for runpod/base derivatives)

## Pod Lifecycle

- Claude spins up pods on demand via MCP tool
- Both on-demand and spot instances supported (setup skill guides the choice)
- Public IP needed for SSH heartbeat, file sync, and download. Heartbeat resolves SSH info in the background — if it never appears, heartbeat logs a warning but `start()` still succeeds. sync/download resolve SSH info on demand (blocks on first call, cached after). Default cloud type is SECURE
- GPU type / image / volume configured via config file (Claude can override)
- Setup skill dynamically helps users configure based on their environment, warns about inconsistent options
- v1: one pod per session. v2: multiple pods

### Pod Creation Retries

- `gpu-type-ids` is an ordered preference list — try each in sequence
- On 500 error: parse the error message to distinguish availability errors from unknown server errors
- Recognized availability error (e.g. "no instances available") → skip to next GPU type immediately
- Other 500 errors → retry up to 3 times with 1s delay, then move to next GPU type
- If all GPU types exhausted: return a clear error to Claude explaining which types were tried and why each failed, so Claude can decide what to try next

## Cleanup

- Three modes: `stop` (preserve pod), `terminate` (delete pod), `disabled` (no automatic cleanup)
- MCP server cleans up pod on graceful shutdown (per config, skipped when disabled)
- Watchdog injected via SSH — background process runs `runpodctl stop pod` (or `remove pod` per cleanup config) if no heartbeat for 5 minutes. Skipped when cleanup is disabled.
- Heartbeat sent by MCP server over SSH — runs in background, does not block `start()`
- When cleanup is disabled: `execute()` output reminds Claude to stop/terminate when done
- Stop hook (bundled in plugin): reads pod ID from state file + verifies via RunPod API. Blocks exit until pod is stopped/terminated per config (skipped when cleanup is disabled)

## Config & State

- Config file (`remote-kernels.toml` at project root), split into two sections:
  - Top-level: fields the MCP server acts on (GPU preference list, image, name, cleanup behavior, budget, notebook directory, sync-include, env vars, startup commands)
  - `[runpod]` section: passed through transparently to the RunPod pod creation API (disk sizes, cloud type, network volume, GPU count, etc.). Extra fields are converted from kebab-case to camelCase and included in the API request. Users can set any RunPod API option here without us needing to explicitly support it.
- Claude can freely edit the config file (e.g. to fall back to a different GPU type)
- `start()` also accepts `gpu_type` and `image` overrides for one-off changes
- State file (`.claude/remote-kernels/state.json`) maintained by MCP server with current pod ID, cleanup mode, and accumulated spend — read by Stop hook
- `.claude/remote-kernels/` auto-creates a `.gitignore` with `*`

## Pod Environment Variables

Three ways to inject env vars into the pod (all optional, later sources override earlier):
- `inherit-env` — list of variable names to forward from the local environment to the pod
- `env-file` — path to a dotenv file to load onto the pod (resolved relative to project root)
- `[env]` section — explicit key-value pairs in the config (for non-secret values)

Required vars (`PUBLIC_KEY`, `JUPYTER_PASSWORD`) are always added last and cannot be overridden.

Setup skill recommends `inherit-env` by default, inspecting the project for relevant services (HuggingFace, W&B, etc.)

## Authentication

- RunPod API key via environment variable `RUNPOD_API_KEY` (with .env support)
- Jupyter token set by MCP server at pod creation
- SSH keypair: ephemeral, generated per pod by MCP server (for rsync and heartbeat)
- Pod has auto-injected `RUNPOD_API_KEY` (pod-scoped) and `RUNPOD_POD_ID` (used by watchdog)

## Budget

- Per-session budget cap via `REMOTE_KERNELS_BUDGET` environment variable (configured in `.claude/settings.json` — Claude Code's built-in guardrails prevent Claude from editing this). Can also be set via `budget-cap` in config file, but env var takes precedence
- **Incompatible with `cleanup: disabled`** — budget enforcement requires the ability to stop/terminate the pod. MCP server rejects this combination at startup.
- MCP server calculates spend as `hourly_rate * uptime`, accumulated across pods in state file. Monotonically increasing per session, never resets.
- `execute()` responses include current spend and remaining budget when a budget is configured
- Two enforcement layers:
  1. MCP server checks budget before `execute`, `create_kernel`, `sync`, and `download`. When exceeded: actively stops/terminates the pod (per cleanup config), then returns error.
  2. Pod-side script (safety net): receives max runtime in seconds at pod start, runs cleanup action when time is up. Recalculated on each pod start. Only active when cleanup is `stop` or `terminate` (not `disabled`, but that combination is rejected anyway).

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

### MCP server improvements

- **Streaming notebook writes**: Currently notebooks are updated only when execution completes (or via `get_output`). Design intent: update the .ipynb as output streams in via the WebSocket, so the notebook reflects intermediate output during long-running executions. Requires changing `Notebook` storage to `Arc<std::sync::Mutex<Notebook>>` so the WS loop callback can safely write to it.
- **Connect to existing pod**: `start(pod_id)` to connect to a pod not created by this session. Challenge: injecting SSH keys into a running pod. Possible approach: fetch Jupyter token from RunPod GraphQL API (`pod.env`), bootstrap SSH key via Jupyter kernel execution. Deferred until there's user demand.
- **End-to-end testing**: Integration tests that exercise the full flow — start pod, create kernel, execute, sync, download, budget enforcement, queuing, get_output, terminate. Currently no automated tests.

### Plugin and distribution

Depends on the MCP server being stable:

- **Plugin shell**: plugin.json, .mcp.json, setup skill, stop hook
- **Setup skill**: should include setting a budget as an explicit step, recommending `REMOTE_KERNELS_BUDGET` in `.claude/settings.json`
- **Binary distribution**: GitHub releases, bootstrap script, platform-specific binaries

## Key Findings from Docs

### RunPod API
- Two APIs: REST (`rest.runpod.io/v1`) and GraphQL (`api.runpod.io/graphql`). REST is newer but GraphQL has features REST doesn't (e.g. `stopAfter`, runtime port mappings)
- **Critical**: REST API does NOT return runtime networking info (port mappings, public IP). Must use GraphQL `pod.runtime.ports` for SSH connection details (ip, publicPort where privatePort=22). SSH is always on a remapped port, never 22 directly
- REST OpenAPI spec at `/v1/openapi.json`, GraphQL schema browser at `graphql-spec.runpod.io`. No official Rust SDK; community crates are immature
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
