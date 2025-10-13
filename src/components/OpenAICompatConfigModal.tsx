import React, { useEffect, useState } from "react";
import { Button } from "./ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/dialog";
import { Input } from "./ui/input";
import { Label } from "./ui/label";
import { Loader2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";

interface OpenAICompatConfigModalProps {
  isOpen: boolean;
  defaultBaseUrl?: string;
  defaultModel?: string;
  onClose: () => void;
  onSubmit: (args: { baseUrl: string; model: string; apiKey?: string; noAuth?: boolean }) => void;
}

export function OpenAICompatConfigModal({
  isOpen,
  defaultBaseUrl = "https://api.openai.com",
  defaultModel = "",
  onClose,
  onSubmit,
}: OpenAICompatConfigModalProps) {
  const [baseUrl, setBaseUrl] = useState(defaultBaseUrl);
  const [model, setModel] = useState(defaultModel);
  const [apiKey, setApiKey] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<null | { ok: boolean; message: string }>(null);

  useEffect(() => {
    if (!isOpen) {
      setBaseUrl(defaultBaseUrl);
      setModel(defaultModel);
      setApiKey("");
      setSubmitting(false);
      setTesting(false);
      setTestResult(null);
    }
  }, [isOpen, defaultBaseUrl, defaultModel]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!baseUrl.trim() || !model.trim()) return;
    try {
      setSubmitting(true);
      const computedNoAuth = apiKey.trim() === "";
      onSubmit({ baseUrl, model, apiKey: computedNoAuth ? undefined : apiKey, noAuth: computedNoAuth });
    } finally {
      // Keep spinner controlled by parent isLoading if needed; here we reset
      setSubmitting(false);
    }
  };

  const handleTest = async () => {
    setTestResult(null);
    setTesting(true);
    try {
      const computedNoAuth = apiKey.trim() === "";
      await invoke("test_openai_endpoint", {
        // Standardize to snake_case for Tauri command args
        base_url: baseUrl.trim(),
        model: model.trim(),
        api_key: computedNoAuth ? undefined : apiKey.trim(),
        no_auth: computedNoAuth,
      });
      setTestResult({ ok: true, message: "Connection successful" });
    } catch (e: any) {
      setTestResult({ ok: false, message: String(e) });
    } finally {
      setTesting(false);
    }
  };

  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogContent className="sm:max-w-[520px]">
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle>Configure OpenAI-Compatible Provider</DialogTitle>
            <DialogDescription>
              Set the API base URL, model ID, and optional API key for any OpenAI-compatible endpoint.
            </DialogDescription>
          </DialogHeader>

          <div className="grid gap-4 py-4">
            <div className="grid gap-2">
              <Label htmlFor="baseUrl">API Base URL</Label>
              <Input
                id="baseUrl"
                placeholder="https://api.openai.com"
                value={baseUrl}
                onChange={(e) => setBaseUrl(e.target.value)}
              />
              <p className="text-xs text-muted-foreground">
                Examples: https://api.openai.com, http://localhost:11434
              </p>
            </div>

            <div className="grid gap-2">
              <Label htmlFor="model">Model ID</Label>
              <Input
                id="model"
                placeholder="e.g. gpt-4o-mini, llama-3.1-8b-instant"
                value={model}
                onChange={(e) => setModel(e.target.value)}
              />
            </div>

            <div className="grid gap-2">
              <Label htmlFor="apiKey">API Key</Label>
              <Input
                id="apiKey"
                type="password"
                placeholder="Leave empty for no authentication"
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
              />
            </div>
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose} disabled={submitting || testing}>
              Cancel
            </Button>
            <Button
              type="button"
              variant="outline"
              onClick={handleTest}
              disabled={!baseUrl.trim() || !model.trim() || submitting || testing}
            >
              {testing ? (
                <>
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Testing...
                </>
              ) : (
                "Test"
              )}
            </Button>
            <Button type="submit" disabled={!baseUrl.trim() || !model.trim() || submitting}>
              {submitting ? (
                <>
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Saving...
                </>
              ) : (
                "Save"
              )}
            </Button>
          </DialogFooter>
          {testResult && (
            <div className={`mt-2 text-sm ${testResult.ok ? "text-green-600" : "text-red-600"}`}>
              {testResult.message}
            </div>
          )}
        </form>
      </DialogContent>
    </Dialog>
  );
}
