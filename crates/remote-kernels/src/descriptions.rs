//! Server-level descriptions used at runtime.
//! Tool descriptions live as doc comments on each tool method in server.rs
//! (rmcp's #[tool] macro reads them automatically).

pub const SERVER_INSTRUCTIONS: &str = "\
MCP server for spinning up cloud GPU instances (RunPod) and interacting with persistent Jupyter kernels. \
Use start() to create a pod, execute() to run Python code, and stop()/terminate() to clean up.\n\
All executions are auto-saved as .ipynb notebook files (path shown in create_kernel output). \
Read these notebooks to recover context after conversation compaction.";
