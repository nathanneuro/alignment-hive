# remote-kernels — Implementation Notes

Notes from the initial implementation session. Details are in the code — this covers what we built, how reality differed from the design, and decisions made along the way.

## What's Built

The Rust MCP server (`crates/remote-kernels/`) is fully functional with all core features:

- **Pod lifecycle**: start, stop, terminate, status
- **Kernel management**: create, shutdown, restart (+ auto-create first kernel on start)
- **Code execution**: Python via Jupyter WebSocket, stateful kernels, timeout handling
- **File sync**: rsync over SSH (both directions), auto-installs rsync on pod
- **Notebooks**: auto-saved .ipynb after every execute(), readable by Claude's Read tool
- **Heartbeat/watchdog**: background process on pod self-stops after 5 min without heartbeat
- **Graceful shutdown**: cleans up pod when MCP server disconnects

## What's NOT Built Yet

From the design doc, still pending:

- **Plugin shell** (plugin.json, .mcp.json for distribution, setup skill, stop hook)
- **Binary distribution** (GitHub releases, bootstrap script, platform-specific binaries)
- **get_output / execution_id** for long-running code (timeout returns a message but no poll mechanism)
- **interrupt tool** (REST endpoint exists in JupyterClient but not wired as an MCP tool)
- **Budget cap enforcement** (tracking shows in status, but no hard enforcement blocking execute)
- **Notebook commit preference** (setup skill would configure this)
- **CLAUDE.md additions** for notebook handling
- **.env support in the MCP server** (currently uses dotenvy to load .env.local, but the design envisions the setup skill managing this)

## Reality vs. Design

### RunPod API
- The REST API returns **500 for transient errors** (machine GPUs taken) with no error codes — just message strings. We retry all 500s up to 3 times.
- The **`gpu` field is absent** from API responses. GPU type lives in `machine.gpuTypeId`, but the `machine` object is only populated in the create response — not in subsequent get_pod calls. We capture GPU name at creation time and store it in memory.
- **rsync is not installed** on the default `runpod/pytorch` image (contrary to what the docs suggest for `runpod/base`). We auto-install it via `apt-get` through the Jupyter kernel during `start()`.
- **Community cloud availability** is hit-or-miss. RTX 3090 and 4090 frequently return resource errors. Having `start()` accept a `gpu_type` override is essential for fallback.

### Config location
- Design said `.claude/remote-kernels/config.json`. We moved config to **`remote-kernels.toml` at project root** — it's user-facing and needs to be visible/editable. State stays in `.claude/remote-kernels/`.
- Used **TOML** instead of JSON for better human readability.

### Tool naming
- Design said `stop()` or `terminate()` dynamically named based on config. We expose **both tools always** — the config default is for automatic cleanup (graceful shutdown, watchdog), but users may want either action regardless of default.

### Execution model
- Design described an `execution_id` system for timed-out executions with `get_output()` polling. We simplified to **timeout returns a message** without a poll mechanism. The WebSocket architecture supports adding this later.
- The WebSocket connection handles **one execution at a time per kernel**. Sending a new execute while one is pending would need queuing (not implemented).

## Design Choices (pending review)

These decisions were made during implementation without explicit user confirmation:

- **Dedicated heartbeat kernel**: the heartbeat uses a separate Jupyter kernel (not visible to the user) to touch `/tmp/heartbeat` every 60s. This avoids interfering with user code execution. Trade-off: uses a small amount of kernel resources on the pod.
- **`start()` accepts `gpu_type` and `image` overrides** via tool parameters. Convenient for fallback when primary GPU is unavailable, but means Claude can choose expensive GPUs without user approval.
- **Notebooks saved to `.claude/remote-kernels/notebooks/`**: this path needs discussion — notebooks are meant for human review but are currently in a hidden directory. May want them somewhere more visible (e.g. `notebooks/` at project root).
- **Notebooks include all execute() calls** including debugging/setup commands (like rsync install checks). May want to filter out "system" executions.
- **SSH info fetched lazily**: public IP and port mappings are only fetched from the RunPod API when `sync()` or `download()` is first called, not during `start()`. This avoids slowing down startup but means the first sync has a brief delay.
- **No interrupt tool exposed**: `JupyterClient::interrupt_kernel()` is implemented but not wired as an MCP tool yet. Would need to be added for the full design.
- **Watchdog uses `runpodctl`** (pre-installed on pods) rather than the RunPod REST API. Simpler and works without network access to the API, but depends on `runpodctl` being available.
- **`.claude/remote-kernels/` needs a `.gitignore`**: the MCP server already creates this directory for state/SSH keys/notebooks — it should also write a `.gitignore` with `*` so none of that gets committed.

## Repo Structure

Added a Cargo workspace alongside the existing bun monorepo:

```
Cargo.toml              # workspace manifest
crates/
  remote-kernels/       # the MCP server
  CLAUDE.md             # Rust dev preferences
remote-kernels.toml     # user-facing config
```

Registered in `.mcp.json` via `cargo run`. For distribution, this will be replaced with a pre-built binary.
