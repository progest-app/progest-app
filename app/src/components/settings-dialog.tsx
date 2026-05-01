import * as React from "react";
import { RefreshCw, Settings, Trash2 } from "lucide-react";
import { toast } from "sonner";

import {
  IpcError,
  aiDeleteKey,
  aiGetConfig,
  aiSetConfig,
  aiSetKey,
  type AiConfigResponse,
} from "@/lib/ipc";
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
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

const AI_PROVIDERS = ["anthropic", "openai"] as const;

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
      toast.success(`API key saved for ${provider}`);
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
      {/* Provider */}
      <div className="grid gap-1.5">
        <Label>Provider</Label>
        <div className="flex gap-1.5">
          {AI_PROVIDERS.map((p) => (
            <Button
              key={p}
              size="sm"
              variant={provider === p ? "default" : "outline"}
              onClick={() => setProvider(p)}
              disabled={saving}
              className="flex-1 capitalize"
            >
              {p}
            </Button>
          ))}
        </div>
      </div>

      {/* Model */}
      <div className="grid gap-1.5">
        <Label htmlFor="ai-model">Model</Label>
        <Input
          id="ai-model"
          value={model}
          onChange={(e) => setModel(e.target.value)}
          placeholder={provider === "anthropic" ? "claude-sonnet-4-20250514" : "gpt-4.1-mini"}
          disabled={saving}
        />
      </div>

      {/* API Key */}
      <div className="grid gap-1.5">
        <Label>API Key</Label>
        {config?.has_key ? (
          <div className="flex items-center gap-2">
            <span className="text-sm text-muted-foreground">Key stored for {config.provider}</span>
            <Button
              size="xs"
              variant="outline"
              className="text-destructive hover:bg-destructive/10"
              onClick={() => void handleDeleteKey()}
              disabled={saving}
            >
              <Trash2 className="mr-1 size-3" />
              Remove
            </Button>
          </div>
        ) : null}
        <div className="flex gap-2">
          <Input
            type="password"
            placeholder={config?.has_key ? "Replace existing key…" : `${provider} API key`}
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
            {saving ? "Saving…" : "Save key"}
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
        <Switch id="audit-log" checked={auditLog} onCheckedChange={setAuditLog} disabled={saving} />
      </div>

      {/* Save config button */}
      <Button onClick={() => void handleSaveConfig()} disabled={saving} className="w-full">
        <RefreshCw className="mr-1.5 size-3.5" />
        Save settings
      </Button>

      {error ? (
        <div className="rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5 text-xs text-destructive">
          {error}
        </div>
      ) : null}
    </div>
  );
}

function GeneralTab() {
  return (
    <div className="grid gap-4">
      <ThemeSettings />
      <div className="pt-2 text-center text-[0.625rem] text-muted-foreground">
        More settings coming in a future release.
      </div>
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
