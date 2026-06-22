export {};

declare global {
  interface Window {
    Sentry?: { captureException: (e: unknown) => void };
  }
}
