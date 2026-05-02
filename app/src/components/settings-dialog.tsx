import * as React from "react";
import { Settings, Trash2 } from "lucide-react";
import { toast } from "sonner";

import {
  IpcError,
  aiDeleteKey,
  aiGetConfig,
  metaGetHidden,
  metaSetHidden,
  aiSetConfig,
  aiSetKey,
  historyGetConfig,
  historySetConfig,
  type AiConfigResponse,
} from "@/lib/ipc";
import { useSettings } from "@/lib/settings-context";
import { Button } from "@/components/ui/button";
import { useTheme } from "next-themes";

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

const AI_PROVIDERS = ["anthropic", "openai"] as const;
const PROVIDER_LABELS: Record<string, string> = {
  anthropic: "Anthropic",
  openai: "OpenAI",
};

type SettingsDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  initialTab?: string;
};

export function SettingsDialog({ open, onOpenChange, initialTab = "ai" }: SettingsDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Settings className="size-4" />
            Settings
          </DialogTitle>
          <DialogDescription>Project-level configuration</DialogDescription>
        </DialogHeader>
        <Tabs defaultValue={initialTab} className="mt-2">
          <TabsList className="w-full">
            <TabsTrigger value="ai" className="flex-1">
              AI
            </TabsTrigger>
            <TabsTrigger value="general" className="flex-1">
              General
            </TabsTrigger>
          </TabsList>
          <TabsContent value="ai" className="mt-4">
            {open ? <AiSettingsTab /> : null}
          </TabsContent>
          <TabsContent value="general" className="mt-4">
            <GeneralTab />
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}

function AiSettingsTab() {
  const { bumpAiConfig } = useSettings();
  const [config, setConfig] = React.useState<AiConfigResponse | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [provider, setProvider] = React.useState<string>("anthropic");
  const [model, setModel] = React.useState("");
  const [auditLog, setAuditLog] = React.useState(true);
  const [keyInput, setKeyInput] = React.useState("");
  const [saving, setSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  const loadConfig = React.useCallback(async () => {
    setLoading(true);
    try {
      const c = await aiGetConfig();
      setConfig(c);
      setProvider(c.provider);
      setModel(c.model);
      setAuditLog(c.audit_log);
    } catch (e) {
      setError(e instanceof IpcError ? e.raw : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void loadConfig();
  }, [loadConfig]);

  const handleSaveKey = async () => {
    if (!keyInput.trim()) return;
    setSaving(true);
    setError(null);
    try {
      await aiSetKey(provider, keyInput.trim());
      await aiSetConfig({ provider });
      setKeyInput("");
      await loadConfig();
      bumpAiConfig();
      toast.success(`API key saved for ${PROVIDER_LABELS[provider] ?? provider}`);
    } catch (e) {
      setError(e instanceof IpcError ? e.raw : String(e));
    } finally {
      setSaving(false);
    }
  };

  const handleDeleteKey = async () => {
    if (!config) return;
    setSaving(true);
    setError(null);
    try {
      await aiDeleteKey(config.provider);
      await loadConfig();
      bumpAiConfig();
      toast.success("API key removed");
    } catch (e) {
      setError(e instanceof IpcError ? e.raw : String(e));
    } finally {
      setSaving(false);
    }
  };

  const handleSaveConfig = async () => {
    setSaving(true);
    setError(null);
    try {
      const opts: Parameters<typeof aiSetConfig>[0] = { provider, audit_log: auditLog };
      if (model) opts.model = model;
      await aiSetConfig(opts);
      await loadConfig();
      bumpAiConfig();
      toast.success("AI settings saved");
    } catch (e) {
      setError(e instanceof IpcError ? e.raw : String(e));
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return <div className="py-4 text-center text-sm text-muted-foreground">Loading…</div>;
  }

  return (
    <div className="grid gap-4">
      {/* Provider + API Key (unified section) */}
      <div className="grid gap-1.5">
        <Label>Provider &amp; API Key</Label>
        {config?.has_key ? (
          <div className="grid gap-2 rounded-md border bg-muted/30 p-3">
            <div className="flex items-center justify-between">
              <span className="text-sm font-medium">
                {PROVIDER_LABELS[config.provider] ?? config.provider}
              </span>
              <span className="text-xs text-emerald-600 dark:text-emerald-400">✓ Key stored</span>
            </div>
            <div className="flex gap-2">
              <Button
                size="xs"
                variant="outline"
                className="text-destructive hover:bg-destructive/10"
                onClick={() => void handleDeleteKey()}
                disabled={saving}
              >
                <Trash2 className="mr-1 size-3" />
                Remove key
              </Button>
            </div>
          </div>
        ) : (
          <div className="grid gap-2 rounded-md border p-3">
            <div className="flex gap-1.5">
              {AI_PROVIDERS.map((p) => (
                <Button
                  key={p}
                  size="sm"
                  variant={provider === p ? "default" : "outline"}
                  onClick={() => setProvider(p)}
                  disabled={saving}
                  className="flex-1"
                >
                  {PROVIDER_LABELS[p] ?? p}
                </Button>
              ))}
            </div>
            <div className="flex gap-2">
              <Input
                type="password"
                placeholder={`${PROVIDER_LABELS[provider] ?? provider} API key`}
                value={keyInput}
                onChange={(e) => setKeyInput(e.target.value)}
                disabled={saving}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void handleSaveKey();
                }}
                className="flex-1"
              />
              <Button
                size="sm"
                onClick={() => void handleSaveKey()}
                disabled={saving || !keyInput.trim()}
              >
                {saving ? "Saving…" : "Save"}
              </Button>
            </div>
          </div>
        )}
      </div>

      {/* Model */}
      <div className="grid gap-1.5">
        <Label htmlFor="ai-model">Model</Label>
        <div className="flex gap-2">
          <Input
            id="ai-model"
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder={provider === "anthropic" ? "claude-sonnet-4-20250514" : "gpt-4.1-mini"}
            disabled={saving}
            className="flex-1"
          />
          <Button
            size="sm"
            variant="outline"
            onClick={() => void handleSaveConfig()}
            disabled={saving}
          >
            {saving ? "Saving…" : "Save"}
          </Button>
        </div>
      </div>

      {/* Audit log */}
      <div className="flex items-center justify-between">
        <div className="grid gap-0.5">
          <Label htmlFor="audit-log">Audit log</Label>
          <span className="text-[0.625rem] text-muted-foreground">
            Log AI requests to .progest/local/ai-log.jsonl
          </span>
        </div>
        <Switch
          id="audit-log"
          checked={auditLog}
          onCheckedChange={(checked) => {
            setAuditLog(checked);
            void aiSetConfig({ audit_log: checked }).then(() => bumpAiConfig());
          }}
          disabled={saving}
        />
      </div>

      {error ? (
        <div className="rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5 text-xs text-destructive">
          {error}
        </div>
      ) : null}
    </div>
  );
}

const DEBOUNCE_KEY = "progest:search-debounce-ms";
const DEBOUNCE_DEFAULT = 400;
const DEBOUNCE_MIN = 100;
const DEBOUNCE_MAX = 1500;
const DEBOUNCE_STEP = 50;

function GeneralTab() {
  const isMac = navigator.platform.includes("Mac");
  return (
    <div className="grid gap-4">
      <ThemeSettings />
      <SearchDebounceSettings />
      <HistoryRetentionSettings />
      <MetaHiddenToggle />
      {isMac ? <MenubarToggle /> : null}
    </div>
  );
}

function MetaHiddenToggle() {
  const [hidden, setHidden] = React.useState(true);
  const [loaded, setLoaded] = React.useState(false);

  React.useEffect(() => {
    void metaGetHidden()
      .then((c) => {
        setHidden(c.hidden);
        setLoaded(true);
      })
      .catch(() => setLoaded(true));
  }, []);

  if (!loaded) return null;

  return (
    <div className="flex items-center justify-between">
      <div className="grid gap-0.5">
        <Label htmlFor="meta-hidden">Hide .meta files</Label>
        <span className="text-[0.625rem] text-muted-foreground">
          Set OS hidden attribute on sidecar files so they don&apos;t appear in Finder / Explorer
        </span>
      </div>
      <Switch
        id="meta-hidden"
        checked={hidden}
        onCheckedChange={(checked) => {
          setHidden(checked);
          void metaSetHidden(checked).catch((e) => {
            toast.error(`Failed: ${e instanceof IpcError ? e.raw : String(e)}`);
          });
        }}
      />
    </div>
  );
}

function MenubarToggle() {
  const [enabled, setEnabled] = React.useState(
    () => localStorage.getItem("progest:show-menubar") === "true",
  );
  return (
    <div className="flex items-center justify-between">
      <div className="grid gap-0.5">
        <Label htmlFor="show-menubar">Show menubar</Label>
        <span className="text-[0.625rem] text-muted-foreground">
          Display a shadcn menubar below the title bar (reload required)
        </span>
      </div>
      <Switch
        id="show-menubar"
        checked={enabled}
        onCheckedChange={(checked) => {
          setEnabled(checked);
          localStorage.setItem("progest:show-menubar", checked ? "true" : "false");
        }}
      />
    </div>
  );
}

function SearchDebounceSettings() {
  const [value, setValue] = React.useState(() => {
    try {
      const v = Number(localStorage.getItem(DEBOUNCE_KEY));
      if (v >= DEBOUNCE_MIN && v <= DEBOUNCE_MAX) return v;
    } catch {}
    return DEBOUNCE_DEFAULT;
  });

  const commit = (next: number) => {
    setValue(next);
    localStorage.setItem(DEBOUNCE_KEY, String(next));
  };

  return (
    <div className="grid gap-1.5">
      <div className="flex items-center justify-between">
        <Label>Search debounce</Label>
        <span className="text-xs tabular-nums text-muted-foreground">{value}ms</span>
      </div>
      <Slider
        min={DEBOUNCE_MIN}
        max={DEBOUNCE_MAX}
        step={DEBOUNCE_STEP}
        value={[value]}
        onValueChange={([v]) => {
          if (v != null) commit(v);
        }}
      />
      <span className="text-[0.625rem] text-muted-foreground">
        Delay before search runs while typing. Higher = less jank, lower = faster results.
      </span>
    </div>
  );
}

function HistoryRetentionSettings() {
  const [retention, setRetention] = React.useState(50);
  const [loaded, setLoaded] = React.useState(false);
  const [saving, setSaving] = React.useState(false);

  React.useEffect(() => {
    void historyGetConfig()
      .then((c) => {
        setRetention(c.retention);
        setLoaded(true);
      })
      .catch(() => setLoaded(true));
  }, []);

  const commit = async (value: number) => {
    setRetention(value);
    setSaving(true);
    try {
      await historySetConfig({ retention: value });
    } catch (e) {
      toast.error(`Failed to save history retention: ${e instanceof IpcError ? e.raw : String(e)}`);
    } finally {
      setSaving(false);
    }
  };

  if (!loaded) return null;

  return (
    <div className="grid gap-1.5">
      <div className="flex items-center justify-between">
        <Label>History retention</Label>
        <span className="text-xs tabular-nums text-muted-foreground">
          {retention} {saving ? "(saving…)" : "entries"}
        </span>
      </div>
      <Slider
        min={10}
        max={500}
        step={10}
        value={[retention]}
        onValueChange={([v]) => {
          if (v != null) void commit(v);
        }}
      />
      <span className="text-[0.625rem] text-muted-foreground">
        Maximum number of undo/redo entries kept. Older entries are evicted automatically.
      </span>
    </div>
  );
}

function ThemeSettings() {
  const { theme, setTheme } = useTheme();

  return (
    <div className="grid gap-1.5">
      <Label>Theme</Label>
      <div className="flex gap-1.5">
        {(["system", "light", "dark"] as const).map((t) => (
          <Button
            key={t}
            size="sm"
            variant={theme === t ? "default" : "outline"}
            onClick={() => setTheme(t)}
            className="flex-1 capitalize"
          >
            {t}
          </Button>
        ))}
      </div>
    </div>
  );
}
