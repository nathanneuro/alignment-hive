# alignment-hive

Shared infrastructure for alignment researchers. [MATS](https://www.matsprogram.org/) fellows are the first intended users, but this is built for the broader AI safety community.

Large orgs benefit from shared tooling and accumulated knowledge across their agents. This project aims to bring some of those advantages to independent researchers:

- **Plugin marketplace** - Curated Claude Code plugins with skills for common research workflows
- **hive-mind** - A system for sharing session learnings across the community (in development)

## Getting Started

### Prerequisites

Install [Claude Code](https://code.claude.com/docs/en/overview) if you haven't already:
```bash
curl -fsSL https://claude.ai/install.sh | bash
```

### 1. Add the marketplace

Run the following commands from within Claude Code (the `/` prefix indicates a Claude Code command):
```
/plugin marketplace add Crazytieguy/alignment-hive
```

Enable auto-update to get new plugins and updates automatically:
1. Run `/plugin`
2. Go to the **Marketplaces** tab
3. Select **alignment-hive**
4. Select **Enable auto-update**
5. Press **Esc** twice to exit the menu

### 2. Install the mats plugin

```
/plugin install mats@alignment-hive
```

### 3. Set up your project

Exit Claude Code and navigate to your project directory:
```
/exit
```
```bash
cd ~/my-project && claude
```

Then run best practices setup:
```
/mats:best-practices
```

This walks you through documentation, plugins, tooling, and permissions — works for both new and existing projects.

## Available Plugins

| Plugin | Description | Install |
|--------|-------------|---------|
| mats | Best practices, fellow handbook, lit review, GitHub Action | `/plugin install mats@alignment-hive` |
| autopilot | Autonomous operation + permission management | `/plugin install autopilot@alignment-hive` |
| llms-fetch-mcp | Documentation fetching with [llms.txt](https://llmstxt.org/) support | `/plugin install llms-fetch-mcp@alignment-hive` |
| remote-kernels | Cloud GPU instances with Jupyter kernels ([RunPod](https://runpod.io)) | `/plugin install remote-kernels@alignment-hive` |
| hive-mind | Session sharing (in development) | `/plugin install hive-mind@alignment-hive` |

## Contributing

The [plugin-dev](https://github.com/anthropics/claude-code-plugins) plugin auto-installs when you clone this repo, so Claude can help with plugin development.

Feedback and suggestions welcome—open an issue, send a Slack DM, or reach out however works for you. All changes go through PR review.

## Web App

A web interface for hive-mind is in development at [alignment-hive.com](https://alignment-hive.com).

## Roadmap

See [docs/roadmap.md](docs/roadmap.md) for what's planned.
