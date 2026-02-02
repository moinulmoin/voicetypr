import { useState, useEffect } from 'react';
import { AlertTriangle, Bug, X, Copy, Check, ExternalLink } from 'lucide-react';
import { open } from '@tauri-apps/plugin-shell';
import { toast } from 'sonner';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  CrashReportData,
  gatherCrashReportData,
  generateGitHubIssueUrl,
} from '@/utils/crashReport';

interface CrashReportDialogProps {
  error: Error;
  componentStack?: string;
  currentModel?: string | null;
  isOpen: boolean;
  onClose: () => void;
  onRetry?: () => void;
}

export function CrashReportDialog({
  error,
  componentStack,
  currentModel,
  isOpen,
  onClose,
  onRetry,
}: CrashReportDialogProps) {
  const [crashData, setCrashData] = useState<CrashReportData | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (isOpen && error) {
      gatherCrashReportData(error, componentStack, currentModel)
        .then(setCrashData)
        .catch(console.error)
        .finally(() => setIsLoading(false));
    }
  }, [isOpen, error, componentStack, currentModel]);

  const handleReportBug = async () => {
    if (!crashData) return;

    try {
      const url = generateGitHubIssueUrl(crashData);
      await open(url);
      toast.success('Opening GitHub in browser...');
    } catch (err) {
      console.error('Failed to open GitHub:', err);
      toast.error('Failed to open browser');
    }
  };

  const handleCopyDetails = async () => {
    if (!crashData) return;

    const details = `Error: ${crashData.errorMessage}
Stack: ${crashData.errorStack || 'N/A'}
App Version: ${crashData.appVersion}
Platform: ${crashData.platform} ${crashData.osVersion}
Architecture: ${crashData.architecture}
Model: ${crashData.currentModel || 'None'}
Timestamp: ${crashData.timestamp}`;

    try {
      await navigator.clipboard.writeText(details);
      setCopied(true);
      toast.success('Details copied to clipboard');
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error('Failed to copy:', err);
      toast.error('Failed to copy details');
    }
  };

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-destructive">
            <AlertTriangle className="h-5 w-5" />
            Something went wrong
          </DialogTitle>
          <DialogDescription>
            An unexpected error occurred. You can report this issue to help us fix it.
          </DialogDescription>
        </DialogHeader>

        {isLoading ? (
          <div className="py-8 text-center text-muted-foreground">
            Gathering crash details...
          </div>
        ) : crashData ? (
          <div className="space-y-4">
            {/* Error Message */}
            <div className="space-y-2">
              <p className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                Error
              </p>
              <div className="rounded-md bg-destructive/10 border border-destructive/20 p-3">
                <p className="text-sm font-mono text-destructive break-all">
                  {crashData.errorMessage}
                </p>
              </div>
            </div>

            {/* Stack Trace (collapsible) */}
            {crashData.errorStack && (
              <div className="space-y-2">
                <p className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                  Stack Trace
                </p>
                <ScrollArea className="h-32 rounded-md border bg-muted/50 p-3">
                  <pre className="text-xs font-mono text-muted-foreground whitespace-pre-wrap break-all">
                    {crashData.errorStack}
                  </pre>
                </ScrollArea>
              </div>
            )}

            {/* System Info */}
            <div className="space-y-2">
              <p className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                System Info
              </p>
              <div className="grid grid-cols-2 gap-2 text-xs">
                <div className="flex justify-between p-2 rounded bg-muted/50">
                  <span className="text-muted-foreground">Version</span>
                  <span className="font-medium">{crashData.appVersion}</span>
                </div>
                <div className="flex justify-between p-2 rounded bg-muted/50">
                  <span className="text-muted-foreground">Platform</span>
                  <span className="font-medium">{crashData.platform}</span>
                </div>
                <div className="flex justify-between p-2 rounded bg-muted/50">
                  <span className="text-muted-foreground">OS</span>
                  <span className="font-medium">{crashData.osVersion}</span>
                </div>
                <div className="flex justify-between p-2 rounded bg-muted/50">
                  <span className="text-muted-foreground">Model</span>
                  <span className="font-medium">{crashData.currentModel || 'None'}</span>
                </div>
              </div>
            </div>
          </div>
        ) : (
          <div className="py-8 text-center text-muted-foreground">
            Failed to gather crash details
          </div>
        )}

        <DialogFooter className="flex-col sm:flex-row gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={handleCopyDetails}
            disabled={!crashData}
            className="gap-2"
          >
            {copied ? <Check className="h-4 w-4" /> : <Copy className="h-4 w-4" />}
            {copied ? 'Copied' : 'Copy Details'}
          </Button>

          <div className="flex gap-2 sm:ml-auto">
            {onRetry && (
              <Button variant="outline" size="sm" onClick={onRetry}>
                Try Again
              </Button>
            )}
            <Button variant="ghost" size="sm" onClick={onClose}>
              <X className="h-4 w-4 mr-1" />
              Dismiss
            </Button>
            <Button
              size="sm"
              onClick={handleReportBug}
              disabled={!crashData}
              className="gap-2"
            >
              <Bug className="h-4 w-4" />
              Report Bug
              <ExternalLink className="h-3 w-3" />
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
