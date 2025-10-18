import { ExternalLink, Loader2 } from 'lucide-react';
import React, { useState } from 'react';
import { Button } from './ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from './ui/dialog';
import { Input } from './ui/input';
import { Label } from './ui/label';

interface ApiKeyModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSubmit: (apiKey: string) => void;
  providerName: string;
  isLoading?: boolean;
  title?: string;
  description?: string;
  submitLabel?: string;
  docsUrl?: string;
}

export function ApiKeyModal({
  isOpen,
  onClose,
  onSubmit,
  providerName,
  isLoading = false,
  title,
  description,
  submitLabel = 'Save API Key',
  docsUrl,
}: ApiKeyModalProps) {
  const [apiKey, setApiKey] = useState('');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (apiKey.trim()) {
      onSubmit(apiKey);
    }
  };

  const handleClose = () => {
    setApiKey('');
    onClose();
  };

  // Clear input when modal closes
  React.useEffect(() => {
    if (!isOpen) {
      setApiKey('');
    }
  }, [isOpen]);

  const getProviderUrl = () => {
    switch (providerName.toLowerCase()) {
      case 'groq':
        return 'https://console.groq.com/keys';
      case 'gemini':
        return 'https://aistudio.google.com/apikey';
      case 'soniox':
        return 'https://soniox.com/docs/stt/get-started';
      default:
        return '';
    }
  };

  const providerUrl = docsUrl ?? getProviderUrl();
  const resolvedTitle = title ?? `Add ${providerName} API Key`;
  const resolvedDescription =
    description ??
    `Enter your API key to enable ${providerName}. Your key is stored securely in the system keychain.`;

  return (
    <Dialog open={isOpen} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-[425px]">
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle>{resolvedTitle}</DialogTitle>
            <DialogDescription>
              {resolvedDescription}
            </DialogDescription>
          </DialogHeader>

          <div className="grid gap-4 py-4">
            <div className="grid gap-2">
              <Label htmlFor="apiKey">API Key</Label>
              <Input
                id="apiKey"
                type="password"
                placeholder={`Enter your ${providerName} API key`}
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                disabled={isLoading}
                autoFocus
              />
            </div>

            {providerUrl && (
              <div className="text-sm text-muted-foreground">
                <a
                  href={providerUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-flex items-center gap-1 hover:underline"
                >
                  Get your {providerName} API key
                  <ExternalLink className="h-3 w-3" />
                </a>
              </div>
            )}
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={handleClose}
              disabled={isLoading}
            >
              Cancel
            </Button>
            <Button
              type="submit"
              disabled={!apiKey.trim() || isLoading}
            >
              {isLoading ? (
                <>
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Saving...
                </>
              ) : (
                submitLabel
              )}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
