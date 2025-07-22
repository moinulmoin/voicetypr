import * as Sentry from '@sentry/react';
import { defaultOptions } from 'tauri-plugin-sentry-api';

// Initialize Sentry with Tauri integration
export function initSentry() {
  // Skip initialization if DSN is not configured
  if (!import.meta.env.VITE_SENTRY_DSN || import.meta.env.VITE_SENTRY_DSN === '__YOUR_SENTRY_DSN__') {
    console.warn('Sentry DSN not configured. Skipping Sentry initialization.');
    return;
  }

  Sentry.init({
    ...defaultOptions,
    dsn: import.meta.env.VITE_SENTRY_DSN,
    integrations: [
      // Browser Tracing for performance monitoring
      // This only tracks performance metrics, not user actions
      Sentry.browserTracingIntegration(),
    ],
    // Performance Monitoring - only tracks load times and API calls
    tracesSampleRate: import.meta.env.DEV ? 1.0 : 0.1,
    // NO Session Replay - respecting user privacy
    // Environment
    environment: import.meta.env.DEV ? 'development' : 'production',
    // Debug mode in development
    debug: import.meta.env.DEV,
    beforeSend(event, hint) {
      // Privacy: Remove any potentially sensitive data
      if (event.request?.cookies) {
        delete event.request.cookies;
      }
      if (event.user) {
        // Only keep anonymous user ID if needed
        event.user = { id: event.user.id };
      }
      
      // Remove any local file paths that might expose user directory structure
      if (event.exception?.values) {
        event.exception.values.forEach(exception => {
          if (exception.stacktrace?.frames) {
            exception.stacktrace.frames.forEach(frame => {
              if (frame.filename && frame.filename.includes('/Users/')) {
                frame.filename = frame.filename.replace(/\/Users\/[^\/]+/, '/Users/[REDACTED]');
              }
            });
          }
        });
      }
      
      // In development, log the error to console as well
      if (event.exception && import.meta.env.DEV) {
        console.error('Sentry captured error:', hint.originalException);
      }
      
      return event;
    },
  });
}

// Export Sentry for use in other parts of the app
export { Sentry };