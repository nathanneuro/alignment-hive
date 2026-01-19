import { useEffect, useState, useRef, useCallback } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import {
  parseSession,
  HiveMindMetaSchema,
  parseKnownEntry,
  type LogicalBlock,
  type HiveMindMeta,
  type KnownEntry,
} from '@alignment-hive/shared';
import { Link } from '@tanstack/react-router';

interface SessionViewerProps {
  url: string;
  sessionId: string;
}

export function SessionViewer({ url, sessionId }: SessionViewerProps) {
  const [data, setData] = useState<{
    meta: HiveMindMeta;
    blocks: LogicalBlock[];
  } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [expandedBlocks, setExpandedBlocks] = useState<Set<number>>(new Set());

  useEffect(() => {
    let cancelled = false;

    async function load() {
      try {
        const response = await fetch(url);
        if (!response.ok) {
          throw new Error(`Failed to fetch: ${response.status}`);
        }

        const text = await response.text();
        const lines = text.split('\n').filter((line) => line.trim());

        if (lines.length === 0) {
          throw new Error('Empty session file');
        }

        // Parse metadata from first line
        const metaLine = JSON.parse(lines[0]);
        const metaResult = HiveMindMetaSchema.safeParse(metaLine);
        if (!metaResult.success) {
          throw new Error('Invalid session metadata');
        }

        // Parse entries
        const entries: KnownEntry[] = [];
        for (let i = 1; i < lines.length; i++) {
          try {
            const raw = JSON.parse(lines[i]);
            const result = parseKnownEntry(raw);
            if (result.data) {
              entries.push(result.data);
            }
          } catch {
            // Skip malformed lines
          }
        }

        const parsed = parseSession(metaResult.data, entries);

        if (!cancelled) {
          setData(parsed);
          setLoading(false);
        }
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : 'Failed to load');
          setLoading(false);
        }
      }
    }

    load();
    return () => {
      cancelled = true;
    };
  }, [url]);

  const toggleExpand = useCallback((index: number) => {
    setExpandedBlocks((prev) => {
      const next = new Set(prev);
      if (next.has(index)) {
        next.delete(index);
      } else {
        next.add(index);
      }
      return next;
    });
  }, []);

  if (loading) {
    return (
      <div className="flex h-96 items-center justify-center rounded-lg border border-border bg-card">
        <div className="text-muted-foreground">Loading session...</div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex h-96 items-center justify-center rounded-lg border border-border bg-card">
        <div className="text-destructive">{error}</div>
      </div>
    );
  }

  if (!data) {
    return null;
  }

  return (
    <div className="rounded-lg border border-border bg-card">
      <div className="border-b border-border px-4 py-2 text-sm text-muted-foreground">
        {data.blocks.length} blocks
      </div>
      <VirtualizedBlockList
        blocks={data.blocks}
        expandedBlocks={expandedBlocks}
        onToggleExpand={toggleExpand}
      />
    </div>
  );
}

interface VirtualizedBlockListProps {
  blocks: LogicalBlock[];
  expandedBlocks: Set<number>;
  onToggleExpand: (index: number) => void;
}

function VirtualizedBlockList({
  blocks,
  expandedBlocks,
  onToggleExpand,
}: VirtualizedBlockListProps) {
  const parentRef = useRef<HTMLDivElement>(null);

  const virtualizer = useVirtualizer({
    count: blocks.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (index) => (expandedBlocks.has(index) ? 320 : 28),
    overscan: 10,
  });

  return (
    <div
      ref={parentRef}
      className="h-[600px] overflow-auto"
      style={{ contain: 'strict' }}
    >
      <div
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          width: '100%',
          position: 'relative',
        }}
      >
        {virtualizer.getVirtualItems().map((virtualItem) => (
          <div
            key={virtualItem.key}
            style={{
              position: 'absolute',
              top: 0,
              left: 0,
              width: '100%',
              height: `${virtualItem.size}px`,
              transform: `translateY(${virtualItem.start}px)`,
            }}
          >
            <BlockRow
              block={blocks[virtualItem.index]}
              index={virtualItem.index}
              isExpanded={expandedBlocks.has(virtualItem.index)}
              onToggle={() => onToggleExpand(virtualItem.index)}
            />
          </div>
        ))}
      </div>
    </div>
  );
}

interface BlockRowProps {
  block: LogicalBlock;
  index: number;
  isExpanded: boolean;
  onToggle: () => void;
}

function BlockRow({ block, index, isExpanded, onToggle }: BlockRowProps) {
  const summary = getBlockSummary(block);
  const typeLabel = getTypeLabel(block);
  const typeColor = getTypeColor(block);

  if (!isExpanded) {
    return (
      <button
        onClick={onToggle}
        className="flex h-7 w-full items-center gap-2 px-4 text-left text-sm hover:bg-muted/50"
      >
        <span className="w-8 shrink-0 text-right font-mono text-xs text-muted-foreground">
          {block.lineNumber}
        </span>
        <span
          className={`w-16 shrink-0 font-mono text-xs font-medium ${typeColor}`}
        >
          {typeLabel}
        </span>
        <span className="truncate text-muted-foreground">{summary}</span>
        {block.type === 'tool' && 'agentId' in block && block.agentId && (
          <Link
            to="/admin/sessions/$sessionId"
            params={{ sessionId: block.agentId }}
            onClick={(e) => e.stopPropagation()}
            className="ml-auto shrink-0 font-mono text-xs text-primary hover:underline"
          >
            {block.agentId.slice(0, 8)}
          </Link>
        )}
      </button>
    );
  }

  return (
    <div className="border-b border-border">
      <button
        onClick={onToggle}
        className="flex h-7 w-full items-center gap-2 bg-muted/50 px-4 text-left text-sm"
      >
        <span className="w-8 shrink-0 text-right font-mono text-xs text-muted-foreground">
          {block.lineNumber}
        </span>
        <span
          className={`w-16 shrink-0 font-mono text-xs font-medium ${typeColor}`}
        >
          {typeLabel}
        </span>
        <span className="truncate text-muted-foreground">{summary}</span>
      </button>
      <div className="max-h-72 overflow-auto bg-muted/25 p-4">
        <BlockContent block={block} />
      </div>
    </div>
  );
}

function BlockContent({ block }: { block: LogicalBlock }) {
  if (block.type === 'tool' && 'toolInput' in block) {
    return (
      <div className="space-y-2 font-mono text-xs">
        <div>
          <div className="mb-1 text-muted-foreground">Input:</div>
          <pre className="whitespace-pre-wrap break-all text-foreground">
            {JSON.stringify(block.toolInput, null, 2)}
          </pre>
        </div>
        {block.toolResult && (
          <div>
            <div className="mb-1 text-muted-foreground">Result:</div>
            <pre className="max-h-40 overflow-auto whitespace-pre-wrap break-all text-foreground">
              {block.toolResult}
            </pre>
          </div>
        )}
      </div>
    );
  }

  if ('content' in block) {
    return (
      <pre className="whitespace-pre-wrap break-words font-mono text-xs text-foreground">
        {block.content}
      </pre>
    );
  }

  return null;
}

function getBlockSummary(block: LogicalBlock): string {
  if (block.type === 'tool' && 'toolName' in block) {
    if (block.toolName === 'Edit' && 'toolInput' in block) {
      const input = block.toolInput as { file_path?: string };
      return input.file_path ?? block.toolName;
    }
    if (block.toolName === 'Read' && 'toolInput' in block) {
      const input = block.toolInput as { file_path?: string };
      return input.file_path ?? block.toolName;
    }
    if (block.toolName === 'Write' && 'toolInput' in block) {
      const input = block.toolInput as { file_path?: string };
      return input.file_path ?? block.toolName;
    }
    if (block.toolName === 'Bash' && 'toolInput' in block) {
      const input = block.toolInput as { command?: string };
      return input.command?.slice(0, 80) ?? block.toolName;
    }
    if (block.toolName === 'Task' && 'toolInput' in block) {
      const input = block.toolInput as {
        subagent_type?: string;
        description?: string;
      };
      return `${input.subagent_type ?? 'Task'}: ${input.description ?? ''}`;
    }
    return block.toolName;
  }

  if (block.type === 'thinking' && 'content' in block) {
    const wordCount = block.content.split(/\s+/).length;
    return `${wordCount} words`;
  }

  if ('content' in block) {
    return truncate(block.content, 100);
  }

  return '';
}

function getTypeLabel(block: LogicalBlock): string {
  if (block.type === 'tool' && 'toolName' in block) {
    return block.toolName.toUpperCase().slice(0, 6);
  }
  return block.type.toUpperCase().slice(0, 6);
}

function getTypeColor(block: LogicalBlock): string {
  switch (block.type) {
    case 'user':
      return 'text-blue-600 dark:text-blue-400';
    case 'assistant':
      return 'text-green-600 dark:text-green-400';
    case 'thinking':
      return 'text-purple-600 dark:text-purple-400';
    case 'tool':
      return 'text-orange-600 dark:text-orange-400';
    case 'system':
      return 'text-gray-600 dark:text-gray-400';
    case 'summary':
      return 'text-cyan-600 dark:text-cyan-400';
    default:
      return 'text-muted-foreground';
  }
}

function truncate(str: string, maxLen: number): string {
  const firstLine = str.split('\n')[0];
  if (firstLine.length <= maxLen) return firstLine;
  return firstLine.slice(0, maxLen - 3) + '...';
}
