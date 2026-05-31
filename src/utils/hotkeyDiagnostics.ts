export interface HotkeyDiagnostics {
  configuredHotkey: string;
  normalizedHotkey: string;
  recordingMode: string;
  useDifferentPttKey: boolean;
  pttHotkey: string | null;
  normalizedPttHotkey: string | null;
  registrationStatus: string;
  registrationError?: string | null;
  lastRegistrationAttemptAt?: string | null;
  lastSuccessfulRegistrationAt?: string | null;
  lastEventAt?: string | null;
  lastEventKind?: string | null;
  lastEventState?: string | null;
  eventCount: number;
  currentRecordingState?: string | null;
  generatedAt: string;
  isRegistered: boolean | null;
}

const REGISTRATION_STATUS_LABELS: Record<string, string> = {
  registered: "Registered",
  failed: "Failed",
  unregistered: "Unregistered",
  restored_after_failure: "Restored previous hotkey after failure",
};

export function formatDiagnosticValue(value: string | number | boolean | null | undefined): string {
  if (value === null || value === undefined || value === "") {
    return "—";
  }
  return String(value);
}

export function formatRegistrationStatus(status: string | null | undefined): string {
  if (!status) {
    return "—";
  }

  return (
    REGISTRATION_STATUS_LABELS[status] ??
    status
      .split("_")
      .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
      .join(" ")
  );
}

export function formatHotkeyDiagnosticLines(diag: HotkeyDiagnostics | null): string[] {
  if (!diag) {
    return ["Hotkey Diagnostics: Unavailable"];
  }

  const lines = [
    `Configured Hotkey: ${formatDiagnosticValue(diag.configuredHotkey)}`,
    `Normalized Hotkey: ${formatDiagnosticValue(diag.normalizedHotkey)}`,
    `Recording Mode: ${formatDiagnosticValue(diag.recordingMode)}`,
    `Registration Status: ${formatRegistrationStatus(diag.registrationStatus)}`,
  ];

  if (diag.registrationError) {
    lines.push(`Registration Error: ${diag.registrationError}`);
  }
  if (diag.isRegistered !== undefined) {
    lines.push(`Is Registered: ${formatDiagnosticValue(diag.isRegistered)}`);
  }
  if (diag.useDifferentPttKey) {
    lines.push(`PTT Hotkey: ${formatDiagnosticValue(diag.pttHotkey)}`);
    lines.push(`Normalized PTT Hotkey: ${formatDiagnosticValue(diag.normalizedPttHotkey)}`);
  }
  if (diag.lastEventAt) {
    lines.push(
      `Last Event: ${formatDiagnosticValue(diag.lastEventKind)} (${formatDiagnosticValue(diag.lastEventState)}) at ${diag.lastEventAt}`
    );
  } else {
    lines.push("Last Event: None detected");
  }
  lines.push(`Event Count: ${diag.eventCount ?? 0}`);
  lines.push(`Current Recording State: ${formatDiagnosticValue(diag.currentRecordingState)}`);
  lines.push(`Generated At: ${formatDiagnosticValue(diag.generatedAt)}`);

  return lines;
}

export function formatHotkeyDiagnosticContext(diag: HotkeyDiagnostics): string {
  return formatHotkeyDiagnosticLines(diag).join("\n");
}

export function formatLastEventSummary(diag: HotkeyDiagnostics | null): string {
  if (!diag?.lastEventAt) {
    return "None detected";
  }
  const kind = formatDiagnosticValue(diag.lastEventKind);
  const state = formatDiagnosticValue(diag.lastEventState);
  return `${kind} (${state})`;
}
