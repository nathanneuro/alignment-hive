#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""
Aggregate unique tool calls from Claude Code session history.

Focuses on:
- Bash commands (command field)
- WebFetch URLs (url field)
- Whether each tool call was denied/blocked

Outputs JSON to stdout.
"""

import json
import os
import re
import sys
from collections import defaultdict
from pathlib import Path


def parse_jsonl_file(path: Path) -> list[dict]:
    """Parse a JSONL file, handling malformed lines gracefully."""
    entries = []
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            for line_num, line in enumerate(f, 1):
                line = line.strip()
                if not line:
                    continue
                try:
                    entries.append(json.loads(line))
                except json.JSONDecodeError:
                    pass  # Skip malformed lines
    except Exception as e:
        print(f"Warning: Could not read {path}: {e}", file=sys.stderr)
    return entries


def extract_tool_calls(entries: list[dict]) -> dict[str, list[dict]]:
    """
    Extract tool calls from session entries.

    Returns dict mapping tool_use_id to tool call info.
    """
    tool_calls = {}

    for entry in entries:
        if entry.get("type") != "assistant":
            continue

        message = entry.get("message", {})
        content = message.get("content", [])

        if isinstance(content, str):
            continue

        for block in content:
            if block.get("type") != "tool_use":
                continue

            tool_id = block.get("id")
            tool_name = block.get("name")
            tool_input = block.get("input", {})

            if not tool_id or not tool_name:
                continue

            tool_calls[tool_id] = {
                "name": tool_name,
                "input": tool_input,
                "timestamp": entry.get("timestamp"),
                "session_id": entry.get("sessionId"),
                "denied": False,  # Will be updated when we find the result
                "denial_reason": None,
            }

    return tool_calls


def find_tool_results(entries: list[dict], tool_calls: dict[str, dict]):
    """
    Find tool results and check if they were denied.
    Updates tool_calls in place.
    """
    denial_patterns = [
        r"blocked",
        r"denied",
        r"not permitted",
        r"permission",
        r"auto-denied",
        r"security policy",
        r"Operation not permitted",
        r"not allowed",
    ]
    denial_regex = re.compile("|".join(denial_patterns), re.IGNORECASE)

    for entry in entries:
        if entry.get("type") != "user":
            continue

        message = entry.get("message", {})
        content = message.get("content", [])

        if isinstance(content, str):
            continue

        for block in content:
            if block.get("type") != "tool_result":
                continue

            tool_id = block.get("tool_use_id")
            if tool_id not in tool_calls:
                continue

            result_content = block.get("content", "")
            if isinstance(result_content, list):
                # Handle structured content
                result_text = " ".join(
                    str(item.get("text", "")) if isinstance(item, dict) else str(item)
                    for item in result_content
                )
            else:
                result_text = str(result_content)

            # Check for denial indicators
            if denial_regex.search(result_text):
                tool_calls[tool_id]["denied"] = True
                # Extract a snippet around the denial keyword
                match = denial_regex.search(result_text)
                if match:
                    start = max(0, match.start() - 50)
                    end = min(len(result_text), match.end() + 100)
                    tool_calls[tool_id]["denial_reason"] = result_text[start:end].strip()


def aggregate_bash_commands(tool_calls: dict[str, dict]) -> dict:
    """Aggregate Bash commands with frequency and denial stats."""
    commands = defaultdict(lambda: {"count": 0, "denied_count": 0, "examples": []})

    for tool_id, call in tool_calls.items():
        if call["name"] != "Bash":
            continue

        command = call["input"].get("command", "")
        if not command:
            continue

        # Extract the base command (first word)
        base_cmd = command.split()[0] if command.split() else command

        # Normalize some common patterns
        # Remove path prefixes for common commands
        if "/" in base_cmd:
            base_cmd = base_cmd.split("/")[-1]

        commands[base_cmd]["count"] += 1
        if call["denied"]:
            commands[base_cmd]["denied_count"] += 1

        # Keep a few examples
        if len(commands[base_cmd]["examples"]) < 5:
            commands[base_cmd]["examples"].append({
                "full_command": command[:200],  # Truncate long commands
                "denied": call["denied"],
                "denial_reason": call["denial_reason"],
            })

    return dict(commands)


def aggregate_webfetch_urls(tool_calls: dict[str, dict]) -> dict:
    """Aggregate WebFetch URLs by domain with frequency and denial stats."""
    domains = defaultdict(lambda: {"count": 0, "denied_count": 0, "example_urls": []})

    for tool_id, call in tool_calls.items():
        if call["name"] != "WebFetch":
            continue

        url = call["input"].get("url", "")
        if not url:
            continue

        # Extract domain
        try:
            from urllib.parse import urlparse
            parsed = urlparse(url)
            domain = parsed.netloc or parsed.path.split("/")[0]
        except Exception:
            domain = url.split("/")[2] if url.count("/") >= 2 else url

        domains[domain]["count"] += 1
        if call["denied"]:
            domains[domain]["denied_count"] += 1

        # Keep a few example URLs
        if len(domains[domain]["example_urls"]) < 3:
            domains[domain]["example_urls"].append({
                "url": url[:200],
                "denied": call["denied"],
            })

    return dict(domains)


def aggregate_other_tools(tool_calls: dict[str, dict]) -> dict:
    """Aggregate other tool usage."""
    tools = defaultdict(lambda: {"count": 0, "denied_count": 0})

    for tool_id, call in tool_calls.items():
        name = call["name"]
        if name in ("Bash", "WebFetch"):
            continue

        tools[name]["count"] += 1
        if call["denied"]:
            tools[name]["denied_count"] += 1

    return dict(tools)


def main():
    claude_projects_dir = Path.home() / ".claude" / "projects"

    if not claude_projects_dir.exists():
        print(f"Error: {claude_projects_dir} does not exist", file=sys.stderr)
        sys.exit(1)

    print(f"Scanning {claude_projects_dir}...", file=sys.stderr)

    all_tool_calls = {}
    session_count = 0
    project_dirs = list(claude_projects_dir.iterdir())

    for project_dir in project_dirs:
        if not project_dir.is_dir():
            continue

        for session_file in project_dir.glob("*.jsonl"):
            session_count += 1
            if session_count % 100 == 0:
                print(f"  Processed {session_count} sessions...", file=sys.stderr)

            entries = parse_jsonl_file(session_file)
            tool_calls = extract_tool_calls(entries)
            find_tool_results(entries, tool_calls)

            # Add project context to each tool call
            project_name = project_dir.name
            for tool_id, call in tool_calls.items():
                call["project"] = project_name

            all_tool_calls.update(tool_calls)

    print(f"Processed {session_count} sessions, found {len(all_tool_calls)} tool calls", file=sys.stderr)

    # Aggregate results
    result = {
        "summary": {
            "total_sessions": session_count,
            "total_tool_calls": len(all_tool_calls),
            "total_projects": len(project_dirs),
        },
        "bash_commands": aggregate_bash_commands(all_tool_calls),
        "webfetch_domains": aggregate_webfetch_urls(all_tool_calls),
        "other_tools": aggregate_other_tools(all_tool_calls),
    }

    # Sort by count
    result["bash_commands"] = dict(
        sorted(result["bash_commands"].items(), key=lambda x: -x[1]["count"])
    )
    result["webfetch_domains"] = dict(
        sorted(result["webfetch_domains"].items(), key=lambda x: -x[1]["count"])
    )
    result["other_tools"] = dict(
        sorted(result["other_tools"].items(), key=lambda x: -x[1]["count"])
    )

    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
