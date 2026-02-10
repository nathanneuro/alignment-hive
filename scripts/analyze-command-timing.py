#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""
Analyze when commands occur within sessions (by percentile).

For each command, shows:
- Total count
- Distribution across session percentiles (early/mid/late)
- Median percentile

Helps identify which commands are most likely to trigger prompts mid-session
(when user may have stepped away) vs early/late (when user is engaged).
"""

import json
import sys
from collections import defaultdict
from pathlib import Path
from statistics import median, mean


def parse_jsonl_file(path: Path) -> list[dict]:
    """Parse a JSONL file, handling malformed lines gracefully."""
    entries = []
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    entries.append(json.loads(line))
                except json.JSONDecodeError:
                    pass
    except Exception as e:
        print(f"Warning: Could not read {path}: {e}", file=sys.stderr)
    return entries


def extract_bash_commands_with_position(entries: list[dict]) -> list[dict]:
    """
    Extract Bash commands with their position in the session.

    Returns list of {command, base_cmd, position_pct} dicts.
    """
    # First, find all tool uses and their order
    tool_uses = []
    for i, entry in enumerate(entries):
        if entry.get("type") != "assistant":
            continue

        message = entry.get("message", {})
        content = message.get("content", [])

        if isinstance(content, str):
            continue

        for block in content:
            if block.get("type") != "tool_use":
                continue

            tool_name = block.get("name")
            tool_input = block.get("input", {})

            tool_uses.append({
                "index": i,
                "name": tool_name,
                "input": tool_input,
            })

    if not tool_uses:
        return []

    # Calculate position as percentile for each Bash command
    total_tools = len(tool_uses)
    results = []

    for pos, tool in enumerate(tool_uses):
        if tool["name"] != "Bash":
            continue

        command = tool["input"].get("command", "")
        if not command:
            continue

        # Extract base command
        base_cmd = command.split()[0] if command.split() else command
        if "/" in base_cmd:
            base_cmd = base_cmd.split("/")[-1]

        # Position as percentage (0-100)
        position_pct = (pos / (total_tools - 1)) * 100 if total_tools > 1 else 50

        results.append({
            "command": command[:200],
            "base_cmd": base_cmd,
            "position_pct": position_pct,
        })

    return results


def main():
    claude_projects_dir = Path.home() / ".claude" / "projects"

    if not claude_projects_dir.exists():
        print(f"Error: {claude_projects_dir} does not exist", file=sys.stderr)
        sys.exit(1)

    print(f"Scanning {claude_projects_dir}...", file=sys.stderr)

    # Collect all commands with their positions
    command_positions = defaultdict(list)  # base_cmd -> list of position_pct
    session_count = 0

    for project_dir in claude_projects_dir.iterdir():
        if not project_dir.is_dir():
            continue

        for session_file in project_dir.glob("*.jsonl"):
            session_count += 1
            if session_count % 100 == 0:
                print(f"  Processed {session_count} sessions...", file=sys.stderr)

            entries = parse_jsonl_file(session_file)
            commands = extract_bash_commands_with_position(entries)

            for cmd in commands:
                command_positions[cmd["base_cmd"]].append(cmd["position_pct"])

    print(f"Processed {session_count} sessions", file=sys.stderr)

    # Analyze each command
    results = []
    for base_cmd, positions in command_positions.items():
        if len(positions) < 5:  # Skip rare commands
            continue

        # Bucket into early (0-33%), mid (33-66%), late (66-100%)
        early = sum(1 for p in positions if p < 33.3)
        mid = sum(1 for p in positions if 33.3 <= p < 66.6)
        late = sum(1 for p in positions if p >= 66.6)
        total = len(positions)

        results.append({
            "command": base_cmd,
            "count": total,
            "median_pct": round(median(positions), 1),
            "mean_pct": round(mean(positions), 1),
            "early_pct": round(early / total * 100, 1),
            "mid_pct": round(mid / total * 100, 1),
            "late_pct": round(late / total * 100, 1),
        })

    # Sort by count
    results.sort(key=lambda x: -x["count"])

    # Print results
    print("\n" + "=" * 90)
    print("COMMAND TIMING ANALYSIS - When commands occur in sessions")
    print("=" * 90)
    print(f"\n{'Command':<15} {'Count':>7} {'Median%':>8} {'Early':>7} {'Mid':>7} {'Late':>7}")
    print("-" * 60)

    for r in results[:50]:  # Top 50
        print(f"{r['command']:<15} {r['count']:>7} {r['median_pct']:>7.1f}% "
              f"{r['early_pct']:>6.1f}% {r['mid_pct']:>6.1f}% {r['late_pct']:>6.1f}%")

    # Group by timing pattern
    print("\n" + "=" * 90)
    print("COMMANDS BY TIMING PATTERN")
    print("=" * 90)

    # Early-heavy (>50% in early third)
    early_heavy = [r for r in results if r["early_pct"] > 50]
    early_heavy.sort(key=lambda x: -x["early_pct"])
    print("\n📊 EARLY-SESSION COMMANDS (>50% in first third):")
    print("   (User is engaged - prompts are low-cost)")
    for r in early_heavy[:15]:
        print(f"   {r['command']:<15} {r['count']:>5}x  early:{r['early_pct']:>5.1f}%")

    # Mid-heavy (>40% in middle third)
    mid_heavy = [r for r in results if r["mid_pct"] > 40]
    mid_heavy.sort(key=lambda x: -x["mid_pct"])
    print("\n⏳ MID-SESSION COMMANDS (>40% in middle third):")
    print("   (User may have stepped away - prompts interrupt flow)")
    for r in mid_heavy[:15]:
        print(f"   {r['command']:<15} {r['count']:>5}x  mid:{r['mid_pct']:>5.1f}%")

    # Late-heavy (>50% in last third)
    late_heavy = [r for r in results if r["late_pct"] > 50]
    late_heavy.sort(key=lambda x: -x["late_pct"])
    print("\n🏁 LATE-SESSION COMMANDS (>50% in last third):")
    print("   (User wrapping up - prompts are low-cost)")
    for r in late_heavy[:15]:
        print(f"   {r['command']:<15} {r['count']:>5}x  late:{r['late_pct']:>5.1f}%")

    # Evenly distributed (all three buckets between 25-40%)
    even = [r for r in results if 25 <= r["early_pct"] <= 45 and 25 <= r["mid_pct"] <= 45 and 25 <= r["late_pct"] <= 45]
    even.sort(key=lambda x: -x["count"])
    print("\n📈 EVENLY DISTRIBUTED COMMANDS (used throughout sessions):")
    print("   (Consider allowing to reduce mid-session prompts)")
    for r in even[:15]:
        print(f"   {r['command']:<15} {r['count']:>5}x  E:{r['early_pct']:>4.0f}% M:{r['mid_pct']:>4.0f}% L:{r['late_pct']:>4.0f}%")

    # JSON output for further analysis
    output = {
        "summary": {
            "total_sessions": session_count,
            "total_commands_analyzed": sum(r["count"] for r in results),
            "unique_commands": len(results),
        },
        "commands": results,
        "patterns": {
            "early_heavy": [r["command"] for r in early_heavy[:15]],
            "mid_heavy": [r["command"] for r in mid_heavy[:15]],
            "late_heavy": [r["command"] for r in late_heavy[:15]],
            "evenly_distributed": [r["command"] for r in even[:15]],
        }
    }

    # Save JSON
    output_path = Path(__file__).parent.parent / "docs" / "data" / "command-timing.json"
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with open(output_path, "w") as f:
        json.dump(output, f, indent=2)
    print(f"\n\nJSON saved to {output_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
