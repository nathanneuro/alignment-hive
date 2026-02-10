#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""
Collect all Claude Code permission settings from projects.

Scans:
- ~/projects/*/.claude/settings.json
- ~/projects/*/.claude/settings.local.json
- ~/projects/*/settings.json (legacy location)
- ~/.claude/settings.json (global)
- ~/.claude/settings.local.json (global)

Outputs JSON to stdout with aggregated permission patterns.
"""

import json
import os
import sys
from collections import defaultdict
from pathlib import Path


def parse_json_file(path: Path) -> dict | None:
    """Parse a JSON file, returning None on failure."""
    try:
        with open(path, "r", encoding="utf-8") as f:
            return json.load(f)
    except Exception as e:
        print(f"Warning: Could not read {path}: {e}", file=sys.stderr)
        return None


def extract_permissions(settings: dict) -> dict:
    """Extract permission settings from a settings dict."""
    permissions = settings.get("permissions", {})
    return {
        "allow": permissions.get("allow", []),
        "deny": permissions.get("deny", []),
    }


def categorize_permission(perm: str) -> tuple[str, str]:
    """
    Categorize a permission string.

    Returns (category, pattern) where:
    - category is like "Bash", "WebFetch", "MCP", "Tool"
    - pattern is the specific pattern within that category
    """
    if perm.startswith("Bash("):
        # Extract the command pattern
        inner = perm[5:-1] if perm.endswith(")") else perm[5:]
        return ("Bash", inner)
    elif perm.startswith("WebFetch("):
        inner = perm[9:-1] if perm.endswith(")") else perm[9:]
        return ("WebFetch", inner)
    elif perm.startswith("WebSearch("):
        inner = perm[10:-1] if perm.endswith(")") else perm[10:]
        return ("WebSearch", inner)
    elif perm.startswith("Read("):
        inner = perm[5:-1] if perm.endswith(")") else perm[5:]
        return ("Read", inner)
    elif perm.startswith("Write("):
        inner = perm[6:-1] if perm.endswith(")") else perm[6:]
        return ("Write", inner)
    elif perm.startswith("Edit("):
        inner = perm[5:-1] if perm.endswith(")") else perm[5:]
        return ("Edit", inner)
    elif perm.startswith("mcp__"):
        # MCP tool permission
        return ("MCP", perm)
    else:
        # Generic tool permission
        return ("Tool", perm)


def main():
    projects_dir = Path.home() / "projects"
    claude_dir = Path.home() / ".claude"

    all_settings = []

    # Check global settings
    for name in ["settings.json", "settings.local.json"]:
        path = claude_dir / name
        if path.exists():
            settings = parse_json_file(path)
            if settings:
                all_settings.append({
                    "path": str(path),
                    "type": "global",
                    "project": None,
                    "settings": settings,
                    "permissions": extract_permissions(settings),
                })

    # Scan projects directory
    if projects_dir.exists():
        for project_dir in projects_dir.iterdir():
            if not project_dir.is_dir():
                continue

            # Check .claude/ subdirectory (current standard)
            claude_subdir = project_dir / ".claude"
            if claude_subdir.exists():
                for name in ["settings.json", "settings.local.json"]:
                    path = claude_subdir / name
                    if path.exists():
                        settings = parse_json_file(path)
                        if settings:
                            all_settings.append({
                                "path": str(path),
                                "type": "project",
                                "project": project_dir.name,
                                "settings": settings,
                                "permissions": extract_permissions(settings),
                            })

            # Check root (legacy location)
            for name in ["settings.json", "settings.local.json"]:
                path = project_dir / name
                if path.exists():
                    settings = parse_json_file(path)
                    if settings:
                        all_settings.append({
                            "path": str(path),
                            "type": "project-legacy",
                            "project": project_dir.name,
                            "settings": settings,
                            "permissions": extract_permissions(settings),
                        })

    print(f"Found {len(all_settings)} settings files", file=sys.stderr)

    # Aggregate permissions across all files
    allow_patterns = defaultdict(lambda: {"count": 0, "projects": []})
    deny_patterns = defaultdict(lambda: {"count": 0, "projects": []})

    for entry in all_settings:
        project = entry["project"] or "global"

        for perm in entry["permissions"]["allow"]:
            category, pattern = categorize_permission(perm)
            key = f"{category}:{pattern}"
            allow_patterns[key]["count"] += 1
            if project not in allow_patterns[key]["projects"]:
                allow_patterns[key]["projects"].append(project)

        for perm in entry["permissions"]["deny"]:
            category, pattern = categorize_permission(perm)
            key = f"{category}:{pattern}"
            deny_patterns[key]["count"] += 1
            if project not in deny_patterns[key]["projects"]:
                deny_patterns[key]["projects"].append(project)

    # Group by category
    allow_by_category = defaultdict(list)
    for key, data in sorted(allow_patterns.items(), key=lambda x: -x[1]["count"]):
        category, pattern = key.split(":", 1)
        allow_by_category[category].append({
            "pattern": pattern,
            "count": data["count"],
            "projects": data["projects"],
        })

    deny_by_category = defaultdict(list)
    for key, data in sorted(deny_patterns.items(), key=lambda x: -x[1]["count"]):
        category, pattern = key.split(":", 1)
        deny_by_category[category].append({
            "pattern": pattern,
            "count": data["count"],
            "projects": data["projects"],
        })

    result = {
        "summary": {
            "total_settings_files": len(all_settings),
            "total_allow_rules": sum(len(e["permissions"]["allow"]) for e in all_settings),
            "total_deny_rules": sum(len(e["permissions"]["deny"]) for e in all_settings),
            "unique_allow_patterns": len(allow_patterns),
            "unique_deny_patterns": len(deny_patterns),
        },
        "allow_by_category": dict(allow_by_category),
        "deny_by_category": dict(deny_by_category),
        "files": [
            {
                "path": e["path"],
                "type": e["type"],
                "project": e["project"],
                "allow": e["permissions"]["allow"],
                "deny": e["permissions"]["deny"],
            }
            for e in all_settings
        ],
    }

    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
