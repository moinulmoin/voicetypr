import { useCallback, useEffect, useRef, useState } from 'react';
import { Bug, Copy, Check, Send } from 'lucide-react';
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
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import {
  gatherManualReportData,
  buildReportBody,
  submitManualReport,
  type ManualReportData,
} from '@/utils/crashReport';
import { useSettings } from '@/contexts/SettingsContext';

interface ReportBugDialogProps {
  isOpen: boolean;
  onClose: () => void;
  initialMessage?: string;
  diagnosticContext?: string;
}

export function ReportBugDialog({
  isOpen,
  onClose,
  initialMessage,
  diagnosticContext,
}: ReportBugDialogProps) {
  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [message, setMessage] = useState('');
  const [messageError, setMessageError] = useState('');
  const [emailError, setEmailError] = useState('');
  const [submitError, setSubmitError] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [copied, setCopied] = useState(false);
  const [fallbackReportData, setFallbackReportData] = useState<ManualReportData | null>(null);
  const { settings } = useSettings();
  const actionIdRef = useRef(0);
  const copiedTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const diagnosticContextRef = useRef(diagnosticContext);

  diagnosticContextRef.current = diagnosticContext;

  useEffect(() => {
    return () => {
      if (copiedTimerRef.current) {
        clearTimeout(copiedTimerRef.current);
      }
    };
  }, []);

  const clearCopyTimer = useCallback(() => {
    if (copiedTimerRef.current) {
      clearTimeout(copiedTimerRef.current);
      copiedTimerRef.current = null;
    }
  }, []);

  const resetSubmitFallback = useCallback(() => {
    setSubmitError('');
    setFallbackReportData(null);
    setCopied(false);
    clearCopyTimer();
  }, [clearCopyTimer]);

  const resetForm = useCallback(() => {
    setName('');
    setEmail('');
    setMessage('');
    setMessageError('');
    setEmailError('');
    setSubmitError('');
    setIsSubmitting(false);
    setCopied(false);
    setFallbackReportData(null);
    clearCopyTimer();
  }, [clearCopyTimer]);

  const handleClose = () => {
    actionIdRef.current += 1;
    resetForm();
    onClose();
  };

  const validate = (): boolean => {
    let isValid = true;

    if (!message.trim()) {
      setMessageError('Please describe the issue you are experiencing.');
      isValid = false;
    } else {
      setMessageError('');
    }

    const trimmedEmail = email.trim();
    if (trimmedEmail && !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(trimmedEmail)) {
      setEmailError('Enter a valid email address or leave it blank.');
      isValid = false;
    } else {
      setEmailError('');
    }

    return isValid;
  };

  useEffect(() => {
    if (isOpen) {
      actionIdRef.current += 1;
      resetForm();
      if (initialMessage) {
        setMessage(initialMessage);
      }
    }
  }, [isOpen, resetForm, initialMessage]);

  const buildAndGather = async (actionId: number): Promise<ManualReportData | null> => {
    resetSubmitFallback();

    try {
      const data = await gatherManualReportData(
        name.trim() || undefined,
        email.trim() || undefined,
        message.trim(),
        settings?.current_model || null,
        diagnosticContextRef.current
      );

      return actionId === actionIdRef.current ? data : null;
    } catch (err) {
      if (actionId === actionIdRef.current) {
        console.error('Failed to gather report data:', err);
        toast.error('Failed to gather report data');
      }
      return null;
    }
  };

  const handleSubmitReport = async () => {
    if (!validate()) return;
    const actionId = actionIdRef.current + 1;
    actionIdRef.current = actionId;
    setIsSubmitting(true);
    const data = await buildAndGather(actionId);
    if (!data) {
      if (actionIdRef.current === actionId) {
        setIsSubmitting(false);
      }
      return;
    }

    try {
      const result = await submitManualReport(data);

      if (actionId !== actionIdRef.current) return;

      if (result.success) {
        toast.success('Report submitted. Thank you.');
        handleClose();
        return;
      }

      setSubmitError(result.message || 'Failed to submit report. You can copy the report and send it manually.');
      setFallbackReportData(data);
      toast.error(result.message || 'Failed to submit report. You can copy the report instead.');
    } finally {
      if (actionId === actionIdRef.current) {
        setIsSubmitting(false);
      }
    }
  };

  const handleCopyReport = async () => {
    if (!fallbackReportData) return;
    const data = fallbackReportData;

    const body = buildReportBody(data);

    try {
      await navigator.clipboard.writeText(body);
      setCopied(true);
      toast.success('Report copied to clipboard');
      clearCopyTimer();
      copiedTimerRef.current = setTimeout(() => {
        setCopied(false);
        copiedTimerRef.current = null;
      }, 2000);
    } catch (err) {
      console.error('Failed to copy report:', err);
      toast.error('Failed to copy report');
    }
  };

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && handleClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Bug className="h-5 w-5" />
            Report a Bug
          </DialogTitle>
          <DialogDescription>
            Tell us what happened. VoiceTypr will include your system info and the
            latest app log excerpt, then submit the report directly to VoiceTypr
            Support.
          </DialogDescription>
        </DialogHeader>

        <form
          className="space-y-4"
          onSubmit={(event) => {
            event.preventDefault();
            void handleSubmitReport();
          }}
          noValidate
        >
          <div className="space-y-2">
            <Label htmlFor="report-name">Name (optional)</Label>
            <Input
              id="report-name"
              placeholder="Your name"
              maxLength={200}
              value={name}
              onChange={(e) => {
                setName(e.target.value);
                if (submitError) resetSubmitFallback();
              }}
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="report-email">Email (optional)</Label>
            <Input
              id="report-email"
              type="email"
              placeholder="your@email.com"
              value={email}
              maxLength={254}
              aria-invalid={Boolean(emailError)}
              aria-describedby={emailError ? 'report-email-error' : undefined}
              onChange={(e) => {
                setEmail(e.target.value);
                if (emailError) setEmailError('');
                if (submitError) resetSubmitFallback();
              }}
            />
            {emailError && (
              <p id="report-email-error" role="alert" className="text-xs text-destructive">
                {emailError}
              </p>
            )}
          </div>

          <div className="space-y-2">
            <Label htmlFor="report-message">
              Message <span className="text-destructive">*</span>
            </Label>
            <Textarea
              id="report-message"
              placeholder="Describe what happened, what you expected, and any steps to reproduce..."
              value={message}
              onChange={(e) => {
                setMessage(e.target.value);
                if (messageError) setMessageError('');
                if (submitError) resetSubmitFallback();
              }}
              rows={5}
              maxLength={5000}
              aria-required="true"
              aria-invalid={Boolean(messageError)}
              aria-describedby={messageError ? 'report-message-error' : undefined}
              className={messageError ? 'border-destructive' : ''}
            />
            {messageError && (
              <p id="report-message-error" role="alert" className="text-xs text-destructive">
                {messageError}
              </p>
            )}
          </div>

          <div className="rounded-md bg-muted/50 border border-border/40 p-3">
            <p className="text-xs text-muted-foreground">
              <strong>What is included:</strong> Your message, optional contact info,
              system info (app version, OS, architecture, model, anonymous device ID),
              {diagnosticContext ? ' additional diagnostics,' : ''}
              and the latest app log excerpt.
            </p>
          </div>
          {submitError && (
            <div className="rounded-md border border-destructive/30 bg-destructive/10 p-3" role="alert">
              <p className="text-xs text-destructive">
                {submitError}
              </p>
              <p className="mt-1 text-xs text-muted-foreground">
                If this keeps happening, copy the prepared report and send it manually.
              </p>
            </div>
          )}


        <DialogFooter className="flex-col sm:flex-row gap-2">
          {fallbackReportData && (
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={handleCopyReport}
              disabled={isSubmitting}
              className="gap-2"
            >
              {copied ? <Check className="h-4 w-4" /> : <Copy className="h-4 w-4" />}
              {copied ? 'Copied' : 'Copy Report'}
            </Button>
          )}

          <div className="flex gap-2 sm:ml-auto">
            <Button type="button" variant="ghost" size="sm" onClick={handleClose} disabled={isSubmitting}>
              Cancel
            </Button>
            <Button
              type="submit"
              size="sm"
              disabled={isSubmitting}
              aria-busy={isSubmitting}
              className="gap-2"
            >
              <Send className="h-4 w-4" />
              {isSubmitting ? 'Submitting...' : 'Submit'}
            </Button>
          </div>
        </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
