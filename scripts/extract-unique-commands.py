#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""
Extract unique bash commands and webfetch domains from session history.

Normalizes commands by removing quoted string contents while preserving structure.
"""

import json
import re
import sys
from pathlib import Path


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
    except Exception:
        pass
    return entries


def normalize_command(cmd: str) -> str:
    """
    Normalize a command by replacing quoted strings with placeholders.

    Examples:
    - python3 -c "print('hello')" -> python3 -c "..."
    - echo "foo bar" -> echo "..."
    - git commit -m 'fix bug' -> git commit -m '...'
    """
    # Replace double-quoted strings (handling escaped quotes)
    normalized = re.sub(r'"(?:[^"\\]|\\.)*"', '"..."', cmd)
    # Replace single-quoted strings
    normalized = re.sub(r"'(?:[^'\\]|\\.)*'", "'...'", normalized)
    # Replace heredocs content (<<EOF ... EOF or <<'EOF' ... EOF)
    normalized = re.sub(r"<<'?(\w+)'?.*?\1", r"<<\1....\1", normalized, flags=re.DOTALL)
    # Collapse multiple spaces
    normalized = re.sub(r'\s+', ' ', normalized)
    # Strip
    normalized = normalized.strip()
    return normalized


def extract_domain(url: str) -> str:
    """Extract domain from URL."""
    try:
        from urllib.parse import urlparse
        parsed = urlparse(url)
        return parsed.netloc or url.split("/")[2] if url.count("/") >= 2 else url
    except Exception:
        if "://" in url:
            return url.split("://")[1].split("/")[0]
        return url.split("/")[0]


def main():
    claude_projects_dir = Path.home() / ".claude" / "projects"

    if not claude_projects_dir.exists():
        print(f"Error: {claude_projects_dir} does not exist", file=sys.stderr)
        sys.exit(1)

    bash_commands = set()
    webfetch_domains = set()
    webfetch_urls = set()

    session_count = 0
    for project_dir in claude_projects_dir.iterdir():
        if not project_dir.is_dir():
            continue

        for session_file in project_dir.glob("*.jsonl"):
            session_count += 1
            if session_count % 200 == 0:
                print(f"  Processed {session_count} sessions...", file=sys.stderr)

            entries = parse_jsonl_file(session_file)

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

                    tool_name = block.get("name")
                    tool_input = block.get("input", {})

                    if tool_name == "Bash":
                        command = tool_input.get("command", "")
                        if command:
                            normalized = normalize_command(command)
                            bash_commands.add(normalized)

                    elif tool_name == "WebFetch":
                        url = tool_input.get("url", "")
                        if url:
                            domain = extract_domain(url)
                            webfetch_domains.add(domain)
                            webfetch_urls.add(url)

    print(f"Processed {session_count} sessions", file=sys.stderr)
    print(f"Found {len(bash_commands)} unique bash commands", file=sys.stderr)
    print(f"Found {len(webfetch_domains)} unique webfetch domains", file=sys.stderr)

    # Output bash commands
    output_dir = Path(__file__).parent.parent / "docs" / "data"
    output_dir.mkdir(parents=True, exist_ok=True)

    with open(output_dir / "unique-bash-commands.txt", "w") as f:
        for cmd in sorted(bash_commands):
            f.write(cmd + "\n")

    with open(output_dir / "unique-webfetch-domains.txt", "w") as f:
        for domain in sorted(webfetch_domains):
            f.write(domain + "\n")

    with open(output_dir / "unique-webfetch-urls.txt", "w") as f:
        for url in sorted(webfetch_urls):
            f.write(url + "\n")

    print(f"Wrote to {output_dir}/unique-bash-commands.txt", file=sys.stderr)
    print(f"Wrote to {output_dir}/unique-webfetch-domains.txt", file=sys.stderr)
    print(f"Wrote to {output_dir}/unique-webfetch-urls.txt", file=sys.stderr)


if __name__ == "__main__":
    main()
