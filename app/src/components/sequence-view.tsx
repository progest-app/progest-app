import * as React from "react";
import { ChevronDown, ChevronRight, Layers } from "lucide-react";

import { filesListAll, type RichSearchHit } from "@/lib/ipc";
import { useProject } from "@/lib/project-context";
import { buildSequenceMap, type SequenceInfo } from "@/lib/sequence-grouping";
import { useDragOutMulti } from "@/lib/use-drag-out";

export function SequenceView(props: { onPickHit?: (hit: RichSearchHit) => void }) {
  const { project, refreshTick } = useProject();
  const [hits, setHits] = React.useState<RichSearchHit[]>([]);
  const [expanded, setExpanded] = React.useState<Set<string>>(() => new Set());

  React.useEffect(() => {
    filesListAll()
      .then(setHits)
      .catch(() => {});
  }, [project?.root, refreshTick]);

  const seqMap = React.useMemo(() => buildSequenceMap(hits), [hits]);
  const sequences = React.useMemo(
    () =>
      [...seqMap.sequences.values()].toSorted((a, b) => a.stemPrefix.localeCompare(b.stemPrefix)),
    [seqMap],
  );

  const membersBySeq = React.useMemo(() => {
    const map = new Map<string, RichSearchHit[]>();
    for (const hit of hits) {
      const id = hit.file_id || hit.path;
      const seqKey = seqMap.hitToSeq.get(id);
      if (!seqKey) continue;
      let list = map.get(seqKey);
      if (!list) {
        list = [];
        map.set(seqKey, list);
      }
      list.push(hit);
    }
    return map;
  }, [hits, seqMap]);

  const toggle = React.useCallback((key: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);

  if (sequences.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-xs text-muted-foreground">
        No sequences detected
      </div>
    );
  }

  return (
    <nav className="h-full overflow-auto p-1 text-xs">
      {sequences.map((seq) => {
        const isOpen = expanded.has(seq.key);
        const members = membersBySeq.get(seq.key) ?? [];
        return (
          <SeqNode
            key={seq.key}
            seq={seq}
            members={members}
            isOpen={isOpen}
            onToggle={() => toggle(seq.key)}
            onPickHit={props.onPickHit}
          />
        );
      })}
    </nav>
  );
}

function SeqNode(props: {
  seq: SequenceInfo;
  members: RichSearchHit[];
  isOpen: boolean;
  onToggle: () => void;
  onPickHit?: ((hit: RichSearchHit) => void) | undefined;
}) {
  const { seq, members, isOpen, onToggle, onPickHit } = props;
  const memberPaths = React.useMemo(() => members.map((m) => m.path), [members]);
  const drag = useDragOutMulti(memberPaths);

  return (
    <div>
      <button
        type="button"
        className="flex w-full items-center gap-1 rounded px-1 py-0.5 hover:bg-accent"
        onClick={onToggle}
        onMouseDown={drag.onMouseDown}
      >
        {isOpen ? (
          <ChevronDown className="size-3 shrink-0 opacity-60" />
        ) : (
          <ChevronRight className="size-3 shrink-0 opacity-60" />
        )}
        <Layers className="size-3 shrink-0 opacity-60" />
        <span className="min-w-0 truncate font-mono">
          {seq.stemPrefix}*.{seq.extension}
        </span>
        <span className="ml-auto shrink-0 text-[0.625rem] text-muted-foreground">{seq.count}</span>
      </button>
      {isOpen ? (
        <div className="ml-4">
          {members.map((hit) => (
            <button
              key={hit.file_id || hit.path}
              type="button"
              className="flex w-full items-center gap-1 rounded px-1 py-0.5 text-left hover:bg-accent"
              onClick={() => onPickHit?.(hit)}
            >
              <span className="min-w-0 truncate font-mono text-muted-foreground">
                {hit.name ?? hit.path.split("/").pop()}
              </span>
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}
