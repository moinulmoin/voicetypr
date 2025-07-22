import { AlertCircle, RefreshCw } from 'lucide-react';
import React from 'react';
import { ErrorBoundary as ReactErrorBoundary } from 'react-error-boundary';
import { Button } from './ui/button';
import * as Sentry from '@sentry/react';

interface ErrorFallbackProps {
  error: Error;
  resetErrorBoundary: () => void;
}

function ErrorFallback({ error, resetErrorBoundary }: ErrorFallbackProps) {
  return (
    <div className="flex flex-col items-center justify-center min-h-[200px] p-8 space-y-4">
      <AlertCircle className="w-12 h-12 text-destructive" />
      <div className="text-center space-y-2">
        <h2 className="text-lg font-semibold">Something went wrong</h2>
        <p className="text-sm text-muted-foreground max-w-md">
          {error.message || 'An unexpected error occurred'}
        </p>
      </div>
      <Button
        onClick={resetErrorBoundary}
        variant="outline"
        size="sm"
        className="gap-2"
      >
        <RefreshCw className="w-4 h-4" />
        Try again
      </Button>
    </div>
  );
}

interface AppErrorBoundaryProps {
  children: React.ReactNode;
  fallback?: React.ComponentType<ErrorFallbackProps>;
  onError?: (error: Error, errorInfo: React.ErrorInfo) => void;
  onReset?: () => void;
  isolate?: boolean;
}

export function AppErrorBoundary({
  children,
  fallback = ErrorFallback,
  onError,
  onReset,
  isolate = true
}: AppErrorBoundaryProps) {
  return (
    <Sentry.ErrorBoundary
      fallback={(errorData) => {
        const FallbackComp = fallback;
        const error = errorData.error instanceof Error ? errorData.error : new Error(String(errorData.error));
        return <FallbackComp error={error} resetErrorBoundary={errorData.resetError} />;
      }}
      beforeCapture={(scope, error) => {
        // Add additional context to Sentry
        scope.setTag('errorBoundary', true);
        scope.setLevel('error');
        // Add error details as extra context
        if (error instanceof Error) {
          scope.setContext('error_details', {
            message: error.message,
            stack: error.stack,
          });
        }
      }}
    >
      <ReactErrorBoundary
        FallbackComponent={fallback}
        onError={(error, errorInfo) => {
          console.error('Error caught by boundary:', error, errorInfo);
          // Report to Sentry manually if needed
          Sentry.withScope((scope) => {
            scope.setContext('errorInfo', { componentStack: errorInfo.componentStack });
            Sentry.captureException(error);
          });
          if (onError) {
            onError(error, errorInfo);
          }
        }}
        onReset={onReset}
        resetKeys={isolate ? [Date.now()] : undefined}
      >
        {children}
      </ReactErrorBoundary>
    </Sentry.ErrorBoundary>
  );
}

// Specific error boundaries for different features
export function RecordingErrorBoundary({ children }: { children: React.ReactNode }) {
  return (
    <AppErrorBoundary
      onError={(error) => {
        console.error('Recording error:', error);
        // Add context for recording errors
        Sentry.withScope((scope) => {
          scope.setTag('feature', 'recording');
          scope.setLevel('error');
          Sentry.captureException(error);
        });
      }}
      onReset={() => {
        // Could reset recording state here
      }}
    >
      {children}
    </AppErrorBoundary>
  );
}

export function SettingsErrorBoundary({ children }: { children: React.ReactNode }) {
  return (
    <AppErrorBoundary
      onError={(error) => {
        console.error('Settings error:', error);
        Sentry.withScope((scope) => {
          scope.setTag('feature', 'settings');
          scope.setLevel('warning');
          Sentry.captureException(error);
        });
      }}
      fallback={({ error, resetErrorBoundary }) => (
        <div className="p-4 border border-destructive/20 rounded-lg bg-destructive/5">
          <p className="text-sm text-destructive">
            Failed to load settings: {error.message}
          </p>
          <Button
            onClick={resetErrorBoundary}
            variant="ghost"
            size="sm"
            className="mt-2"
          >
            Retry
          </Button>
        </div>
      )}
    >
      {children}
    </AppErrorBoundary>
  );
}

export function ModelManagementErrorBoundary({ children }: { children: React.ReactNode }) {
  return (
    <AppErrorBoundary
      onError={(error) => {
        console.error('Model management error:', error);
        Sentry.withScope((scope) => {
          scope.setTag('feature', 'model_management');
          scope.setLevel('error');
          Sentry.captureException(error);
        });
      }}
      fallback={({ resetErrorBoundary }) => (
        <div className="text-center p-4 space-y-2">
          <p className="text-sm text-muted-foreground">
            Error loading models
          </p>
          <Button onClick={resetErrorBoundary} size="sm" variant="outline">
            Reload Models
          </Button>
        </div>
      )}
    >
      {children}
    </AppErrorBoundary>
  );
}

