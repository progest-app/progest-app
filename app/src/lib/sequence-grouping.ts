import type { RichSearchHit } from "@/lib/ipc";

export type SequenceInfo = {
  key: string;
  stemPrefix: string;
  extension: string;
  parentDir: string;
  rangeStart: number;
  rangeEnd: number;
  count: number;
};

export type SequenceMap = {
  sequences: Map<string, SequenceInfo>;
  hitToSeq: Map<string, string>;
  seqFirstHit: Map<string, string>;
};

const SEQ_RE = /^(.+?)(\d+)$/;

function parseSeqName(hit: RichSearchHit): {
  stem: string;
  number: number;
  ext: string;
  parentDir: string;
} | null {
  const name = hit.name ?? hit.path.split("/").pop() ?? "";
  const dotIdx = name.lastIndexOf(".");
  if (dotIdx <= 0) return null;
  const base = name.slice(0, dotIdx);
  const ext = name.slice(dotIdx + 1);
  const m = SEQ_RE.exec(base);
  if (!m) return null;
  const lastSlash = hit.path.lastIndexOf("/");
  const parentDir = lastSlash >= 0 ? hit.path.slice(0, lastSlash) : "";
  return { stem: m[1]!, number: Number.parseInt(m[2]!, 10), ext, parentDir };
}

export function buildSequenceMap(hits: RichSearchHit[], minMembers = 2): SequenceMap {
  const groups = new Map<
    string,
    { stem: string; ext: string; parentDir: string; numbers: number[]; hitIds: string[] }
  >();

  for (const hit of hits) {
    const parsed = parseSeqName(hit);
    if (!parsed) continue;
    const key = `${parsed.parentDir}\0${parsed.stem}\0${parsed.ext}`;
    let group = groups.get(key);
    if (!group) {
      group = {
        stem: parsed.stem,
        ext: parsed.ext,
        parentDir: parsed.parentDir,
        numbers: [],
        hitIds: [],
      };
      groups.set(key, group);
    }
    group.numbers.push(parsed.number);
    group.hitIds.push(hit.file_id || hit.path);
  }

  const sequences = new Map<string, SequenceInfo>();
  const hitToSeq = new Map<string, string>();
  const seqFirstHit = new Map<string, string>();

  for (const [key, group] of groups) {
    if (group.hitIds.length < minMembers) continue;
    sequences.set(key, {
      key,
      stemPrefix: group.stem,
      extension: group.ext,
      parentDir: group.parentDir,
      rangeStart: Math.min(...group.numbers),
      rangeEnd: Math.max(...group.numbers),
      count: group.hitIds.length,
    });
    for (const id of group.hitIds) {
      hitToSeq.set(id, key);
    }
    seqFirstHit.set(key, group.hitIds[0]!);
  }

  return { sequences, hitToSeq, seqFirstHit };
}
