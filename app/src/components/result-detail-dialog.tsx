import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { RichSearchHit } from "@/lib/ipc";

export function ResultDetailDialog(props: {
  hit: RichSearchHit | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { hit, open, onOpenChange } = props;
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-xl">
        <DialogHeader>
          <DialogTitle className="font-mono text-sm break-all">{hit?.path ?? ""}</DialogTitle>
          <DialogDescription>{hit?.kind ?? ""}</DialogDescription>
        </DialogHeader>
        {hit ? <Detail hit={hit} /> : null}
      </DialogContent>
    </Dialog>
  );
}

function Detail({ hit }: { hit: RichSearchHit }) {
  return (
    <div className="grid gap-3 text-xs">
      <Row label="file_id" value={hit.file_id} />
      {hit.name ? <Row label="name" value={hit.name} /> : null}
      {hit.ext ? <Row label="ext" value={hit.ext} /> : null}
      <Row
        label="tags"
        value={hit.tags.length > 0 ? hit.tags.map((t) => `#${t}`).join(" ") : "—"}
      />
      <ViolationsRow counts={hit.violations} />
      {hit.custom_fields.length > 0 ? (
        <div>
          <div className="mb-1 text-muted-foreground">custom fields</div>
          <ul className="grid gap-1 pl-3">
            {hit.custom_fields.map((f) => (
              <li key={f.key} className="font-mono">
                <span className="text-muted-foreground">{f.key}</span>:{" "}
                <span>{String(f.value)}</span>
                <span className="ml-1 text-[0.625rem] text-muted-foreground">({f.type})</span>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid grid-cols-[6rem_1fr] items-baseline gap-2">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-mono break-all">{value}</span>
    </div>
  );
}

function ViolationsRow({
  counts,
}: {
  counts: { naming: number; placement: number; sequence: number };
}) {
  const total = counts.naming + counts.placement + counts.sequence;
  return (
    <div className="grid grid-cols-[6rem_1fr] items-baseline gap-2">
      <span className="text-muted-foreground">violations</span>
      <span className="font-mono">
        {total === 0
          ? "—"
          : `naming:${counts.naming} placement:${counts.placement} sequence:${counts.sequence}`}
      </span>
    </div>
  );
}
