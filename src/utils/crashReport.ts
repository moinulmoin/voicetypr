import { getVersion } from '@tauri-apps/api/app';
import { platform, version as osVersion, arch } from '@tauri-apps/plugin-os';
import { invoke } from '@tauri-apps/api/core';
import { getModelDisplayName } from '@/lib/model-display';
import { createLogger } from "@/lib/logger";

const log = createLogger("crash-report");

export interface SystemSpecs {
  osName: string;
  osVersion: string;
  kernelVersion: string;
  arch: string;
  cpuBrand: string;
  cpuCores: number;
  totalMemoryMb: number;
  gpus: string[];
}

export interface CrashReportData {
  errorMessage: string;
  errorStack?: string;
  componentStack?: string;
  appVersion: string;
  platform: string;
  osVersion: string;
  architecture: string;
  currentModel: string | null;
  deviceId: string;
  timestamp: string;
  logFileName: string | null;
  logContent: string;
  logTruncated: boolean;
  logStatusNote: string;
  systemSpecs?: SystemSpecs;
}

export async function gatherCrashReportData(
  error: Error,
  componentStack?: string,
  currentModel?: string | null
): Promise<CrashReportData> {
  // Get async values
  const [appVer, deviceId, logAttachment, systemSpecs] = await Promise.all([
    getVersion().catch(() => 'Unknown'),
    invoke<string>('get_device_id').catch(() => 'Unknown'),
    getLatestLogAttachment(),
    invoke<SystemSpecs>('get_system_specs').catch(() => undefined),
  ]);

  // Get sync values from OS plugin (these are not promises)
  let os = 'Unknown';
  let osVer = 'Unknown';
  let architecture = 'Unknown';

  try {
    os = platform();
    osVer = osVersion();
    architecture = arch();
  } catch (e) {
    log.error('Failed to get OS info:', e);
  }

  return {
    errorMessage: error.message || 'Unknown error',
    errorStack: error.stack,
    componentStack: componentStack,
    appVersion: appVer,
    platform: os,
    osVersion: osVer,
    architecture,
    currentModel: currentModel || null,
    deviceId,
    timestamp: new Date().toISOString(),
    logFileName: logAttachment.fileName,
    logContent: logAttachment.redactedContent,
    logTruncated: logAttachment.truncated,
    logStatusNote: logAttachment.statusNote,
    systemSpecs,
  };
}



export interface ManualReportData {
  name?: string;
  email?: string;
  message: string;
  appVersion: string;
  platform: string;
  osVersion: string;
  architecture: string;
  currentModel: string | null;
  deviceId: string;
  timestamp: string;
  logFileName: string | null;
  logContent: string;
  logTruncated: boolean;
  logStatusNote: string;
  systemSpecs?: SystemSpecs;
}

interface LatestLogAttachment {
  fileName: string | null;
  redactedContent: string;
  truncated: boolean;
  statusNote: string;
}

async function getLatestLogAttachment(): Promise<LatestLogAttachment> {
  return invoke<LatestLogAttachment>('get_latest_log_for_bug_report').catch(() => ({
    fileName: null,
    redactedContent: '',
    truncated: false,
    statusNote: 'Failed to retrieve log.',
  }));
}

export async function gatherManualReportData(
  name: string | undefined,
  email: string | undefined,
  message: string,
  currentModel?: string | null
): Promise<ManualReportData> {
  const [appVer, deviceId, logAttachment, systemSpecs] = await Promise.all([
    getVersion().catch(() => 'Unknown'),
    invoke<string>('get_device_id').catch(() => 'Unknown'),
    getLatestLogAttachment(),
    invoke<SystemSpecs>('get_system_specs').catch(() => undefined),
  ]);

  let os = 'Unknown';
  let osVer = 'Unknown';
  let architecture = 'Unknown';

  try {
    os = platform();
    osVer = osVersion();
    architecture = arch();
  } catch (e) {
    log.error('Failed to get OS info:', e);
  }

  return {
    name,
    email,
    message,
    appVersion: appVer,
    platform: os,
    osVersion: osVer,
    architecture,
    currentModel: currentModel || null,
    deviceId,
    timestamp: new Date().toISOString(),
    logFileName: logAttachment.fileName,
    logContent: logAttachment.redactedContent,
    logTruncated: logAttachment.truncated,
    logStatusNote: logAttachment.statusNote,
    systemSpecs,
  };
}

export function buildReportBody(data: ManualReportData): string {
  const parts: string[] = [];

  parts.push('## Voicetypr Support Report');
  parts.push('');

  if (data.name || data.email) {
    parts.push('### Contact');
    if (data.name) parts.push(`Name: ${data.name}`);
    if (data.email) parts.push(`Email: ${data.email}`);
    parts.push('');
  }

  parts.push('### Message');
  parts.push(data.message);
  parts.push('');

  parts.push('## Environment');
  parts.push('');
  parts.push('| Property | Value |');
  parts.push('|----------|-------|');
  parts.push(`| App Version | ${data.appVersion} |`);
  parts.push(`| Platform | ${data.platform} |`);
  parts.push(`| OS Version | ${data.osVersion} |`);
  parts.push(`| Architecture | ${data.architecture} |`);
  parts.push(`| Current Model | ${getModelDisplayName(data.currentModel) || 'None'} |`);
  parts.push(`| Device ID | ${data.deviceId} |`);
  parts.push(`| Timestamp | ${data.timestamp} |`);
  parts.push('');
  if (data.systemSpecs) {
    const specs = data.systemSpecs;
    parts.push('## System');
    parts.push('');
    parts.push('| Property | Value |');
    parts.push('|----------|-------|');
    parts.push(`| OS | ${specs.osName} ${specs.osVersion} |`);
    parts.push(`| Kernel | ${specs.kernelVersion} |`);
    parts.push(`| CPU | ${specs.cpuBrand} (${specs.cpuCores} cores) |`);
    parts.push(`| Memory | ${Math.round(specs.totalMemoryMb / 1024)} GB |`);
    parts.push(`| GPU | ${specs.gpus.length ? specs.gpus.join(', ') : 'Unknown'} |`);
    parts.push('');
  }


  // Latest log section
  if (data.logContent) {
    parts.push('## Latest App Log');
    parts.push('');
    if (data.logTruncated) {
      parts.push('_The log was truncated. Only the most recent entries are included._');
    }
    if (data.logFileName) {
      parts.push(`_Source: ${data.logFileName}_`);
    }
    parts.push('');
    parts.push('```');
    parts.push(data.logContent);
    parts.push('```');
    parts.push('');
    parts.push('_Log content has been automatically redacted for common sensitive patterns._');
  } else if (data.logStatusNote) {
    parts.push('## Latest App Log');
    parts.push('');
    parts.push(`> ${data.logStatusNote}`);
  }

  parts.push('');
  parts.push('---');
  parts.push('_This report was generated by the Voicetypr Report Bug feature._');

  return parts.join('\n');
}

const BUG_REPORT_ENDPOINT =
  import.meta.env.VITE_BUG_REPORT_ENDPOINT || 'https://voicetypr.com/api/v1/bug-reports';

interface ReportEnvironmentPayload {
  appVersion: string;
  platform: string;
  osVersion: string;
  architecture: string;
  currentModel?: string | null;
  deviceId: string;
  timestamp: string;
  systemSpecs?: SystemSpecs;
}

interface LatestLogPayload {
  fileName: string | null;
  content: string;
  truncated: boolean;
  statusNote: string;
}

export type BugReportPayload =
  | {
      kind: 'manual';
      name?: string;
      email?: string;
      message: string;
      environment: ReportEnvironmentPayload;
      latestLog: LatestLogPayload;
    }
  | {
      kind: 'crash';
      message?: string;
      crash: {
        errorMessage: string;
        errorStack?: string;
        componentStack?: string;
      };
      environment: ReportEnvironmentPayload;
      latestLog: LatestLogPayload;
    };

export interface ReportSubmitResult {
  success: boolean;
  message: string;
}

export function buildManualReportPayload(data: ManualReportData): BugReportPayload {
  return {
    kind: 'manual',
    name: data.name,
    email: data.email,
    message: data.message,
    environment: buildEnvironmentPayload(data),
    latestLog: buildLatestLogPayload(data),
  };
}

export function buildCrashReportPayload(data: CrashReportData): BugReportPayload {
  return {
    kind: 'crash',
    crash: {
      errorMessage: data.errorMessage,
      errorStack: data.errorStack,
      componentStack: data.componentStack,
    },
    environment: buildEnvironmentPayload(data),
    latestLog: buildLatestLogPayload(data),
  };
}

export async function submitManualReport(data: ManualReportData): Promise<ReportSubmitResult> {
  return submitBugReport(buildManualReportPayload(data));
}

export async function submitCrashReport(data: CrashReportData): Promise<ReportSubmitResult> {
  return submitBugReport(buildCrashReportPayload(data));
}

interface ApiResponseBody {
  success?: boolean;
  message?: string;
}

const BUG_REPORT_TIMEOUT_MS = 10_000;


async function submitBugReport(payload: BugReportPayload): Promise<ReportSubmitResult> {
  try {
    const response = await fetch(BUG_REPORT_ENDPOINT, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
      signal: AbortSignal.timeout(BUG_REPORT_TIMEOUT_MS),
    });

    const responseBody = await response.json().catch(() => null) as ApiResponseBody | null;

    if (!response.ok || responseBody?.success === false) {
      return {
        success: false,
        message: responseBody?.message || 'Failed to submit report.',
      };
    }

    return {
      success: true,
      message: responseBody?.message || 'Report submitted.',
    };
  } catch (error) {
    log.error('Failed to submit bug report:', error);
    return {
      success: false,
      message: 'Could not connect to Voicetypr Support. Please use Copy Report instead.',
    };
  }
}

function buildEnvironmentPayload(data: ManualReportData | CrashReportData): ReportEnvironmentPayload {
  const environment: ReportEnvironmentPayload = {
    appVersion: data.appVersion,
    platform: data.platform,
    osVersion: data.osVersion,
    architecture: data.architecture,
    currentModel: data.currentModel,
    deviceId: data.deviceId,
    timestamp: data.timestamp,
  };

  if (data.systemSpecs) {
    environment.systemSpecs = data.systemSpecs;
  }

  return environment;
}

function buildLatestLogPayload(data: ManualReportData | CrashReportData): LatestLogPayload {
  return {
    fileName: data.logFileName,
    content: data.logContent,
    truncated: data.logTruncated,
    statusNote: data.logStatusNote,
  };
}
