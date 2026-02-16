# /// script
# requires-python = ">=3.11"
# dependencies = ["httpx", "aiofiles", "markdownify>=0.13.1", "pymupdf4llm", "pymupdf"]
# ///
"""
Pipeline: download PDFs, convert to markdown, and process LW/AF posts incrementally.

As each PDF downloads, it's immediately queued for markdown conversion.
LW/AF posts with HTML content are converted in parallel.
Completed markdown file paths are printed to stdout as they finish.

PDF conversion uses Marker (marker-pdf) when available for high-quality output
with academic papers. Falls back to pymupdf4llm if Marker is not installed.

Usage:
    uv run process_papers_pipeline.py \
        --input deduplicated.json \
        --output-dir papers/ \
        [--max-downloads 5]
"""

import argparse
import asyncio
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import aiofiles
import httpx
from markdownify import markdownify as md

MAX_CONCURRENT_DOWNLOADS = 5
MAX_RETRIES = 5
TIMEOUT_SECONDS = 120

HAS_MARKER = shutil.which("marker_single") is not None


# --- Filename/ID helpers ---


def sanitize_filename(name: str) -> str:
    name = re.sub(r'[<>:"/\\|?*]', "", name)
    name = re.sub(r"\s+", "_", name)
    return name[:100]


def get_paper_id(paper: dict) -> str:
    if paper.get("doi"):
        return sanitize_filename(paper["doi"].replace("/", "_"))
    if paper.get("arxiv_id"):
        arxiv_id = paper["arxiv_id"]
        if "arxiv.org" in arxiv_id:
            arxiv_id = arxiv_id.split("/")[-1]
        return sanitize_filename(f"arxiv_{arxiv_id}")
    if paper.get("post_id"):
        return sanitize_filename(f"lw_{paper['post_id']}")
    if paper.get("id"):
        return sanitize_filename(paper["id"])
    if paper.get("paperId"):
        return sanitize_filename(f"s2_{paper['paperId']}")
    title = paper.get("title", "unknown")
    return sanitize_filename(title[:50])


def get_pdf_url(paper: dict) -> str | None:
    if paper.get("pdf_url"):
        return paper["pdf_url"]
    if paper.get("openAccessPdf"):
        oa = paper["openAccessPdf"]
        if isinstance(oa, dict):
            return oa.get("url")
        return oa
    return None


# --- Marker PDF conversion ---


def convert_pdf_with_marker(pdf_path: Path, md_output_path: Path) -> bool:
    """Convert a PDF to markdown using marker_single.

    Marker outputs to a subdirectory (output_dir/stem/stem.md). We convert
    into a temp dir, then move the .md file to the expected flat location.
    """
    with tempfile.TemporaryDirectory() as tmpdir:
        try:
            result = subprocess.run(
                [
                    "marker_single",
                    str(pdf_path),
                    tmpdir,
                    "--disable_image_extraction",
                ],
                capture_output=True,
                text=True,
                timeout=300,
            )
            if result.returncode != 0:
                print(
                    f"  marker failed for {pdf_path.name}: {result.stderr[:200]}",
                    file=sys.stderr,
                )
                return False

            # Find the generated markdown (marker creates tmpdir/stem/stem.md)
            md_files = list(Path(tmpdir).rglob("*.md"))
            if not md_files:
                print(
                    f"  marker produced no output for {pdf_path.name}",
                    file=sys.stderr,
                )
                return False

            # Move the first .md file to the expected location
            md_output_path.write_text(md_files[0].read_text(encoding="utf-8"), encoding="utf-8")
            return True

        except subprocess.TimeoutExpired:
            print(f"  marker timed out for {pdf_path.name}", file=sys.stderr)
            return False
        except FileNotFoundError:
            print(
                "  ERROR: marker_single not found. Install with: uv tool install marker-pdf",
                file=sys.stderr,
            )
            return False
        except Exception as e:
            print(f"  Error converting {pdf_path.name}: {e}", file=sys.stderr)
            return False


# --- Fallback PDF conversion (pymupdf4llm) ---


def convert_pdf_with_pymupdf(pdf_path: Path, md_output_path: Path) -> bool:
    """Convert a PDF to markdown using pymupdf4llm (fallback when Marker unavailable)."""
    try:
        import pymupdf4llm

        md_text = pymupdf4llm.to_markdown(
            str(pdf_path), write_images=False, embed_images=False
        )
        md_output_path.write_text(md_text, encoding="utf-8")
        return True
    except Exception as e:
        print(f"  Error converting {pdf_path.name}: {e}", file=sys.stderr)
        return False


def convert_pdf(pdf_path: Path, md_output_path: Path) -> bool:
    """Convert PDF using Marker if available, otherwise pymupdf4llm."""
    if HAS_MARKER:
        return convert_pdf_with_marker(pdf_path, md_output_path)
    return convert_pdf_with_pymupdf(pdf_path, md_output_path)


# --- LW/AF HTML conversion ---


def slugify(text: str) -> str:
    text = text.lower()
    text = re.sub(r"[^\w\s-]", "", text)
    text = re.sub(r"[-\s]+", "-", text).strip("-")
    return text[:80]


def format_comment(comment: dict, indent_level: int = 0) -> str:
    prefix = "#" * (3 + indent_level)
    author = comment.get("author", "Anonymous")
    score = comment.get("score", "?")
    content = comment.get("html_content") or comment.get("content", "")
    if content.startswith("<"):
        content = md(content, strip=["script", "style"])
    lines = [f"{prefix} {author} (score: {score})", "", content, ""]
    for reply in comment.get("replies", []):
        lines.append(format_comment(reply, indent_level + 1))
    return "\n".join(lines)


def convert_lw_post(post: dict) -> str:
    title = post.get("title", "Untitled")
    author = post.get("author", "Unknown")
    date = post.get("date", post.get("published_date", "Unknown"))
    score = post.get("score", "?")
    url = post.get("url", "")
    html_content = post.get("html_content", "")
    comments = post.get("comments", [])
    main_content = md(html_content, strip=["script", "style"]) if html_content else ""
    lines = [
        f"# {title}",
        "",
        f"**Author:** {author}",
        f"**Posted:** {date}",
        f"**Score:** {score}",
        f"**URL:** {url}",
        "",
        "---",
        "",
        main_content,
        "",
    ]
    if comments:
        lines.extend(["---", "", f"## Comments ({len(comments)} comments)", ""])
        for comment in comments:
            lines.append(format_comment(comment))
    return "\n".join(lines)


# --- Pipeline ---


async def download_pdf(
    client: httpx.AsyncClient, url: str, output_path: Path, paper_id: str
) -> bool:
    for attempt in range(MAX_RETRIES):
        try:
            resp = await client.get(url, follow_redirects=True, timeout=TIMEOUT_SECONDS)
            resp.raise_for_status()
            content_type = resp.headers.get("content-type", "")
            if "pdf" not in content_type.lower() and not url.endswith(".pdf"):
                if "html" in content_type.lower():
                    return False
            async with aiofiles.open(output_path, "wb") as f:
                await f.write(resp.content)
            return True
        except httpx.HTTPStatusError as e:
            if e.response.status_code in (403, 404, 451):
                return False
            if attempt < MAX_RETRIES - 1:
                await asyncio.sleep(2**attempt)
        except Exception as e:
            if attempt < MAX_RETRIES - 1:
                await asyncio.sleep(2**attempt)
            else:
                print(f"    Failed to download {paper_id}: {e}", file=sys.stderr)
    return False


async def run_pipeline(
    papers: list[dict],
    output_dir: Path,
    max_downloads: int,
) -> dict:
    output_dir.mkdir(parents=True, exist_ok=True)

    stats = {
        "total": len(papers),
        "downloaded": 0,
        "download_skipped_no_url": 0,
        "download_skipped_exists": 0,
        "download_failed": 0,
        "converted_pdf": 0,
        "converted_lw": 0,
        "convert_failed": 0,
        "convert_skipped": 0,
        "ready_files": [],
    }

    # Separate LW/AF posts (HTML conversion) from PDF papers
    lw_posts = []
    pdf_papers = []
    for paper in papers:
        source = paper.get("source", "").lower()
        if source in (
            "lesswrong",
            "alignment_forum",
            "alignmentforum",
            "ea_forum",
        ) and paper.get("html_content"):
            lw_posts.append(paper)
        else:
            pdf_papers.append(paper)

    # Queue for PDF paths waiting to be converted by Marker
    convert_queue: asyncio.Queue[Path | None] = asyncio.Queue()

    loop = asyncio.get_event_loop()
    download_semaphore = asyncio.Semaphore(max_downloads)

    async def download_and_enqueue(paper: dict) -> None:
        paper_id = get_paper_id(paper)
        pdf_url = get_pdf_url(paper)

        if not pdf_url:
            stats["download_skipped_no_url"] += 1
            return

        pdf_path = output_dir / f"{paper_id}.pdf"
        md_path = output_dir / f"{paper_id}.md"

        # Skip if markdown already exists
        if md_path.exists():
            stats["convert_skipped"] += 1
            stats["ready_files"].append(str(md_path))
            print(str(md_path), flush=True)
            return

        if pdf_path.exists():
            stats["download_skipped_exists"] += 1
            await convert_queue.put(pdf_path)
            return

        async with download_semaphore:
            async with httpx.AsyncClient() as client:
                success = await download_pdf(client, pdf_url, pdf_path, paper_id)

        if success:
            stats["downloaded"] += 1
            print(f"  Downloaded: {paper_id}", file=sys.stderr)
            await convert_queue.put(pdf_path)
        else:
            stats["download_failed"] += 1

    async def converter_worker() -> None:
        """Pull PDFs from the queue and convert to markdown (sequential).

        Runs one at a time (Marker loads ML models per invocation; pymupdf4llm
        is CPU-bound). Still pipelines with downloads — conversion starts as
        soon as the first PDF is ready rather than waiting for all downloads.
        """
        while True:
            pdf_path = await convert_queue.get()
            if pdf_path is None:
                convert_queue.task_done()
                break

            md_path = pdf_path.with_suffix(".md")
            if md_path.exists():
                stats["convert_skipped"] += 1
                stats["ready_files"].append(str(md_path))
                print(str(md_path), flush=True)
                convert_queue.task_done()
                continue

            # Run conversion in a thread to avoid blocking the event loop
            success = await loop.run_in_executor(
                None, convert_pdf, pdf_path, md_path
            )

            if success:
                stats["converted_pdf"] += 1
                stats["ready_files"].append(str(md_path))
                print(str(md_path), flush=True)
                print(
                    f"  Converted: {pdf_path.name} → {md_path.name}", file=sys.stderr
                )
            else:
                stats["convert_failed"] += 1

            convert_queue.task_done()

    def process_lw_posts() -> None:
        for post in lw_posts:
            title = post.get("title", "untitled")
            paper_id = post.get("id", slugify(title))
            md_path = output_dir / f"{paper_id}.md"

            if md_path.exists():
                stats["convert_skipped"] += 1
                stats["ready_files"].append(str(md_path))
                print(str(md_path), flush=True)
                continue

            markdown_content = convert_lw_post(post)
            md_path.write_text(markdown_content)
            stats["converted_lw"] += 1
            stats["ready_files"].append(str(md_path))
            print(str(md_path), flush=True)
            print(f"  Converted LW/AF: {title[:50]}...", file=sys.stderr)

    # --- Run everything ---
    print(
        f"Pipeline: {len(pdf_papers)} PDFs to download/convert, {len(lw_posts)} LW/AF posts",
        file=sys.stderr,
    )

    # Single converter worker (Marker is GPU/CPU-heavy, no benefit from parallelism)
    converter_task = asyncio.create_task(converter_worker())

    # Process LW/AF posts in a thread (fast, no async needed)
    lw_future = loop.run_in_executor(None, process_lw_posts)

    # Download all PDFs (each enqueues conversion on completion)
    download_tasks = [download_and_enqueue(paper) for paper in pdf_papers]
    await asyncio.gather(*download_tasks)

    # Signal converter worker to stop
    await convert_queue.put(None)

    # Wait for all conversions to finish
    await converter_task

    # Wait for LW/AF processing
    await lw_future

    return stats


def main():
    parser = argparse.ArgumentParser(
        description="Pipeline: download, convert, and process papers"
    )
    parser.add_argument(
        "--input",
        type=Path,
        required=True,
        help="JSON file with paper metadata",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        required=True,
        help="Directory for PDFs and markdown",
    )
    parser.add_argument(
        "--max-downloads",
        type=int,
        default=MAX_CONCURRENT_DOWNLOADS,
        help="Max concurrent downloads (default: 5)",
    )
    args = parser.parse_args()

    if HAS_MARKER:
        print("Using Marker for PDF conversion (high quality)", file=sys.stderr)
    else:
        print("Marker not found, using pymupdf4llm fallback", file=sys.stderr)
        print("For better quality: uv tool install marker-pdf", file=sys.stderr)

    with open(args.input) as f:
        papers = json.load(f)

    if not isinstance(papers, list):
        print(
            "Error: input file must contain a JSON array of papers", file=sys.stderr
        )
        sys.exit(1)

    stats = asyncio.run(run_pipeline(papers, args.output_dir, args.max_downloads))

    # Print summary to stderr
    print("", file=sys.stderr)
    print("Pipeline Summary:", file=sys.stderr)
    print(f"  Total papers: {stats['total']}", file=sys.stderr)
    print(f"  PDFs downloaded: {stats['downloaded']}", file=sys.stderr)
    print(f"  PDFs already existed: {stats['download_skipped_exists']}", file=sys.stderr)
    print(f"  No PDF URL: {stats['download_skipped_no_url']}", file=sys.stderr)
    print(f"  Download failed: {stats['download_failed']}", file=sys.stderr)
    converter_name = "Marker" if HAS_MARKER else "pymupdf4llm"
    print(f"  PDFs converted ({converter_name}): {stats['converted_pdf']}", file=sys.stderr)
    print(f"  LW/AF posts converted: {stats['converted_lw']}", file=sys.stderr)
    print(f"  Conversion failed: {stats['convert_failed']}", file=sys.stderr)
    print(f"  Already had markdown: {stats['convert_skipped']}", file=sys.stderr)
    print(f"  Total ready files: {len(stats['ready_files'])}", file=sys.stderr)

    # Save stats
    stats_path = args.output_dir / "pipeline_stats.json"
    with open(stats_path, "w") as f:
        json.dump(stats, f, indent=2)
    print(f"  Stats saved to: {stats_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
