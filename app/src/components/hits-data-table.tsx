import * as React from "react";
import {
  flexRender,
  getCoreRowModel,
  getSortedRowModel,
  useReactTable,
  type ColumnDef,
  type ColumnSizingState,
  type SortingState,
  type VisibilityState,
} from "@tanstack/react-table";
import { ArrowDown, ArrowUp, ArrowUpDown, ChevronDown, FileIcon } from "lucide-react";

import { FileContextMenu } from "@/components/file-context-menu";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Button } from "@/components/ui/button";
import { ViolationBadges } from "@/components/violation-badges";
import type { RichSearchHit } from "@/lib/ipc";
import { cn } from "@/lib/utils";

function basename(hit: RichSearchHit): string {
  return hit.name ?? hit.path.split("/").pop() ?? hit.path;
}

/** Column ids exposed for sort persistence. Keep stable. */
export type HitsColumnId = "path" | "filename" | "tags" | "violations" | "kind" | "ext";

/**
 * TanStack-table-backed list view for `RichSearchHit`s.
 *
 * The component is fully controlled — sorting and column visibility
 * live in the FlatView so the same sort state can drive the grid
 * mode's render path. Pagination, row selection, and column filters
 * are intentionally not enabled: DSL queries already scope the data
 * server-side, and 10k+ unfiltered hit sets are vanishingly rare in
 * practice (still feasible without virtualization given the small
 * row height).
 */
export function HitsDataTable(props: {
  hits: RichSearchHit[];
  onPick: ((hit: RichSearchHit) => void) | undefined;
  sorting: SortingState;
  onSortingChange: (next: SortingState) => void;
  columnVisibility: VisibilityState;
  onColumnVisibilityChange: (next: VisibilityState) => void;
  columnSizing: ColumnSizingState;
  onColumnSizingChange: (next: ColumnSizingState) => void;
}) {
  const columns = React.useMemo<ColumnDef<RichSearchHit>[]>(
    () => [
      {
        id: "filename",
        accessorFn: basename,
        header: ({ column }) => <SortHeader column={column}>Filename</SortHeader>,
        cell: ({ row }) => (
          <div className="flex items-center gap-2 truncate">
            <FileIcon className="size-3.5 shrink-0 opacity-60" />
            <span className="truncate font-mono">{basename(row.original)}</span>
          </div>
        ),
        sortingFn: (a, b) => basename(a.original).localeCompare(basename(b.original)),
        size: 240,
        minSize: 100,
      },
      {
        id: "path",
        accessorKey: "path",
        header: ({ column }) => <SortHeader column={column}>Path</SortHeader>,
        cell: ({ row }) => <span className="truncate font-mono">{row.original.path}</span>,
        sortingFn: (a, b) => a.original.path.localeCompare(b.original.path),
        size: 300,
        minSize: 100,
      },
      {
        id: "tags",
        accessorFn: (h) => h.tags.join(" "),
        header: ({ column }) => <SortHeader column={column}>Tags</SortHeader>,
        cell: ({ row }) =>
          row.original.tags.length > 0 ? (
            <span className="text-[0.625rem] text-muted-foreground">
              {row.original.tags.map((t) => `#${t}`).join(" ")}
            </span>
          ) : (
            <span className="text-muted-foreground">—</span>
          ),
        sortingFn: (a, b) => a.original.tags.join(" ").localeCompare(b.original.tags.join(" ")),
        size: 160,
        minSize: 60,
      },
      {
        id: "violations",
        accessorFn: (h) => h.violations.naming + h.violations.placement + h.violations.sequence,
        header: ({ column }) => <SortHeader column={column}>Violations</SortHeader>,
        cell: ({ row }) => {
          const v = row.original.violations;
          if (v.naming + v.placement + v.sequence === 0) {
            return <span className="text-muted-foreground">—</span>;
          }
          return <ViolationBadges counts={v} className="" />;
        },
        size: 120,
        minSize: 60,
      },
      {
        id: "kind",
        accessorKey: "kind",
        header: ({ column }) => <SortHeader column={column}>Kind</SortHeader>,
        cell: ({ row }) => <span className="text-muted-foreground">{row.original.kind}</span>,
        size: 80,
        minSize: 50,
      },
      {
        id: "ext",
        accessorKey: "ext",
        header: ({ column }) => <SortHeader column={column}>Ext</SortHeader>,
        cell: ({ row }) => (
          <span className="font-mono text-muted-foreground">{row.original.ext ?? ""}</span>
        ),
        sortingFn: (a, b) => (a.original.ext ?? "").localeCompare(b.original.ext ?? ""),
        size: 70,
        minSize: 40,
      },
    ],
    [],
  );

  const table = useReactTable({
    data: props.hits,
    columns,
    state: {
      sorting: props.sorting,
      columnVisibility: props.columnVisibility,
      columnSizing: props.columnSizing,
    },
    columnResizeMode: "onChange",
    enableColumnResizing: true,
    onSortingChange: (updater) => {
      const next = typeof updater === "function" ? updater(props.sorting) : updater;
      props.onSortingChange(next);
    },
    onColumnVisibilityChange: (updater) => {
      const next = typeof updater === "function" ? updater(props.columnVisibility) : updater;
      props.onColumnVisibilityChange(next);
    },
    onColumnSizingChange: (updater) => {
      const next = typeof updater === "function" ? updater(props.columnSizing) : updater;
      props.onColumnSizingChange(next);
    },
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
  });

  return (
    <Table style={{ width: table.getTotalSize(), tableLayout: "fixed" }}>
      <TableHeader>
        {table.getHeaderGroups().map((headerGroup) => (
          <TableRow key={headerGroup.id}>
            {headerGroup.headers.map((header) => (
              <TableHead
                key={header.id}
                className="relative text-[0.625rem] uppercase tracking-wide"
                style={{ width: header.getSize() }}
              >
                {header.isPlaceholder
                  ? null
                  : flexRender(header.column.columnDef.header, header.getContext())}
                {header.column.getCanResize() ? (
                  <div
                    onMouseDown={header.getResizeHandler()}
                    onTouchStart={header.getResizeHandler()}
                    className={cn(
                      "absolute right-0 top-0 h-full w-1 cursor-col-resize select-none touch-none",
                      header.column.getIsResizing()
                        ? "bg-primary"
                        : "bg-transparent hover:bg-border",
                    )}
                  />
                ) : null}
              </TableHead>
            ))}
          </TableRow>
        ))}
      </TableHeader>
      <TableBody>
        {table.getRowModel().rows.length === 0 ? (
          <TableRow>
            <TableCell
              colSpan={table.getAllLeafColumns().length}
              className="h-16 text-center text-muted-foreground"
            >
              No results.
            </TableCell>
          </TableRow>
        ) : (
          table.getRowModel().rows.map((row) => (
            <FileContextMenu
              key={row.original.file_id || row.original.path}
              path={row.original.path}
            >
              <TableRow className="cursor-pointer" onClick={() => props.onPick?.(row.original)}>
                {row.getVisibleCells().map((cell) => (
                  <TableCell
                    key={cell.id}
                    className="overflow-hidden text-xs"
                    style={{ width: cell.column.getSize() }}
                  >
                    {flexRender(cell.column.columnDef.cell, cell.getContext())}
                  </TableCell>
                ))}
              </TableRow>
            </FileContextMenu>
          ))
        )}
      </TableBody>
    </Table>
  );
}

/**
 * Toolbar dropdown that exposes a checkbox per data column so the
 * user can hide / show columns. Mounted by FlatView next to the
 * list/grid toggle.
 */
export function ColumnVisibilityMenu(props: {
  columnVisibility: VisibilityState;
  onColumnVisibilityChange: (next: VisibilityState) => void;
}) {
  const labels: Record<HitsColumnId, string> = {
    path: "Path",
    filename: "Filename",
    tags: "Tags",
    violations: "Violations",
    kind: "Kind",
    ext: "Extension",
  };
  const ids: HitsColumnId[] = ["filename", "path", "tags", "violations", "kind", "ext"];
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline" size="sm" className="h-8">
          Columns <ChevronDown />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        {ids.map((id) => {
          const visible = props.columnVisibility[id] !== false;
          return (
            <DropdownMenuCheckboxItem
              key={id}
              checked={visible}
              disabled={id === "filename"}
              onCheckedChange={(next) => {
                props.onColumnVisibilityChange({
                  ...props.columnVisibility,
                  [id]: Boolean(next),
                });
              }}
            >
              {labels[id]}
            </DropdownMenuCheckboxItem>
          );
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/**
 * Sortable column header — clicking cycles asc → desc → unset.
 * Tanstack provides the cycle implementation; we just wire a button
 * around it with an arrow that reflects the current state.
 */
function SortHeader({
  column,
  children,
}: {
  column: { getIsSorted: () => "asc" | "desc" | false; toggleSorting: (desc?: boolean) => void };
  children: React.ReactNode;
}) {
  const sorted = column.getIsSorted();
  return (
    <button
      type="button"
      onClick={() => column.toggleSorting(sorted === "asc")}
      className={cn(
        "flex items-center gap-1 text-[0.625rem] uppercase tracking-wide hover:text-foreground",
        sorted ? "text-foreground" : "text-muted-foreground",
      )}
    >
      {children}
      {sorted === "asc" ? (
        <ArrowUp className="size-3" />
      ) : sorted === "desc" ? (
        <ArrowDown className="size-3" />
      ) : (
        <ArrowUpDown className="size-3 opacity-50" />
      )}
    </button>
  );
}
