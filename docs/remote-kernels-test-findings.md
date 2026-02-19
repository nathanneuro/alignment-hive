# remote-kernels MCP Server: Test Findings

Testing session run against the MCP server as loaded. Goal: document what works and what doesn't. No fixes applied.

Reference: `docs/gpu-tool-design.md`

---

## BLOCKER — `start()` times out waiting for Jupyter

`start()` consistently fails with:
> "Jupyter failed to start: Jupyter server did not become ready after 3 minutes"

This happened on both COMMUNITY and SECURE cloud. The pod reaches RUNNING state and stays there, but `start()` errors out before Jupyter is accessible. As a result, all execution, sync, and download tests could not be completed.

Context: an earlier prototype of the MCP server was starting pods successfully (including COMMUNITY pods). This is a regression — something changed in the Jupyter connection/readiness logic.

---

## Issues Found

| # | Tool | Expected | Actual |
|---|------|----------|--------|
| B1 | `start()` | Pod starts, Jupyter ready, returns pod info | Errors: "Jupyter failed to start: Jupyter server did not become ready after 3 minutes" — on both COMMUNITY and SECURE cloud. Pod IS running on RunPod, Jupyter is not reachable. Regression from earlier working prototype. |
| B2 | `status()` after `stop()` | Shows stopped pod (with storage cost note) | Says "No pod is currently running" — stopped pods are invisible |
| B3 | `terminate()` after `stop()` | Should terminate the stopped pod | Errors: "No pod is running. Call start() first." — can't clean up a stopped pod via MCP |

---

## What Works

| Tool | Result |
|------|--------|
| `status()` with no pod | Clean message: "No pod is currently running" |
| `stop()` | Works, returns session cost, clear message |
| `terminate()` on running pod | Works, returns session cost, confirms deletion |
| `execute()` / `create_kernel()` with no pod | Clean error: "No pod is running. Call start() first." |
| `sync(include=["../path"])` | Correctly rejected with clear validation error |
| `sync(include=["/absolute/path"])` | Correctly rejected with clear validation error |

---

## Tests Not Run (blocked by B1)

All execution, file, and kernel management features — steps covering `create_kernel()`, `execute()` (all variants), `get_output()`, `interrupt()`, `restart_kernel()`, `shutdown_kernel()`, notebook file creation, `sync()`, and `download()`.

---

## Fixes Applied

### B1: `dockerStartCmd` causes container restart loop

**Root cause**: `dockerStartCmd` overrides the RunPod image's Docker CMD, which is responsible for starting services (Jupyter, SSH) and keeping the container alive. The container runs the startup command, exits, and restart-loops.

**Fix**: Removed `dockerStartCmd`. Startup commands (rsync install + user commands) run via SSH after the pod is up. `sync()` and `download()` also ensure rsync is installed before each call.

### B2 + B3: Stopped pods invisible to `status()` and `terminate()`

**Root cause**: `stop()` cleared the pod_id from both memory and disk, making the pod unreachable.

**Fix**: `stop()` preserves pod_id (and reconnection credentials) on disk. `status()` and `terminate()` fall back to disk when in-memory state is empty.

### GraphQL null deserialization

`runtime.ports` can be `null` (not just missing) in GraphQL responses, which crashed deserialization. Fixed.

### Pod reconnection across sessions

`start()` now reconnects to a pod from a previous session if one exists — resumes stopped pods, reconnects to running pods. Credentials (Jupyter token, SSH key path) are persisted in state.json. If `start()` is called with gpu_type/image overrides, it requires terminating the existing pod first.

---

## End-to-End Test Plan (next session)

Run against the live MCP server (requires session restart to reload the binary):

1. `start()` — pod creates, Jupyter ready
2. `create_kernel()` → `execute()` — run Python code
3. `status()` — shows running pod
4. `sync()` — rsync works
5. `stop()` → `status()` — shows stopped pod
6. `terminate()` → `status()` — shows "no pod running"
7. `start()` again — creates fresh pod (no stale state)
8. Stop session, start new session, `start()` — reconnects to existing pod
