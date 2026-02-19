# remote-kernels crate

MCP server for spinning up cloud GPU instances and interacting with persistent Jupyter kernels.

## Publishing

Don't publish or release without asking.

1. Bump version in `Cargo.toml`
2. Bump `plugins/remote-kernels/.claude-plugin/plugin.json` to match (the bootstrap script uses this to download the right binary)
3. Update README.md if needed
4. Commit the version bumps and `Cargo.lock`
5. `git tag vX.Y.Z && git push origin vX.Y.Z`
6. GitHub Actions builds binaries and creates a GitHub Release automatically
