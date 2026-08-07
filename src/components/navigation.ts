import type { LucideIcon } from "lucide-react";
import {
  Bug,
  Clock,
  Cpu,
  FileAudio,
  Home,
  Keyboard,
  Key,
  Layers,
  Settings2,
  Share2,
  Sparkles,
  Terminal,
} from "lucide-react";

export type ScreenId =
  | "overview"
  | "recordings"
  | "audio"
  | "general"
  | "shortcuts"
  | "models"
  | "network"
  | "formatting"
  | "license"
  | "agent"
  | "advanced"
  | "report-problem";

export interface ScreenDefinition {
  id: ScreenId;
  label: string;
  icon: LucideIcon;
  description: string;
}


export const primaryScreens: ScreenDefinition[] = [
  {
    id: "overview",
    label: "Overview",
    icon: Home,
    description: "Readiness, recent activity, and quick next steps.",
  },
  {
    id: "recordings",
    label: "History",
    icon: Clock,
    description: "Past transcriptions and retry actions.",
  },
  {
    id: "audio",
    label: "Upload",
    icon: FileAudio,
    description: "Transcribe existing audio files.",
  },
  {
    id: "models",
    label: "Models",
    icon: Cpu,
    description: "Local models, cloud providers, and remote Voicetypr servers.",
  },
  {
    id: "network",
    label: "Network sharing",
    icon: Share2,
    description: "Share this device's transcription engine over your network, or connect to one.",
  },
  {
    id: "formatting",
    label: "Polish",
    icon: Sparkles,
    description: "Clean up your dictation automatically, plus always-on text rules.",
  },
  {
    id: "general",
    label: "Settings",
    icon: Settings2,
    description: "Hotkeys, paste behavior, microphones, and app preferences.",
  },
  {
    id: "shortcuts",
    label: "Shortcuts",
    icon: Keyboard,
    description: "Recording, history, and mode shortcuts.",
  },
  {
    id: "license",
    label: "Account",
    icon: Key,
    description: "Trial status, license activation, and account access.",
  },
  {
    id: "agent",
    label: "Agent & CLI",
    icon: Terminal,
    description: "Drive Voicetypr from scripts and agents via the CLI and local API.",
  },
];

export const secondaryScreens: ScreenDefinition[] = [
  {
    id: "advanced",
    label: "Advanced",
    icon: Layers,
    description: "Power-user and diagnostics settings.",
  },
  {
    id: "report-problem",
    label: "Report a problem",
    icon: Bug,
    description: "Send an issue with diagnostic logs.",
  },
];


export const screens = [...primaryScreens, ...secondaryScreens] as const;

export const isScreenId = (value: string): value is ScreenId =>
  screens.some((screen) => screen.id === value);

const screenById = (id: ScreenId): ScreenDefinition =>
  screens.find((screen) => screen.id === id) as ScreenDefinition;

const everydayScreens = [
  screenById("overview"),
  screenById("recordings"),
  screenById("audio"),
  screenById("general"),
  screenById("shortcuts"),
  screenById("models"),
  screenById("formatting"),
];

const powerUserScreens = [
  screenById("network"),
  screenById("agent"),
];

const accountScreen = screenById("license");

export const reportProblemNavScreens: ScreenDefinition[] = [
  screenById("report-problem"),
];

export const powerUserUtilityNavScreens: ScreenDefinition[] = [
  screenById("advanced"),
  screenById("report-problem"),
];

export const recommendedNavScreens: ScreenDefinition[] = [
  ...everydayScreens,
  accountScreen,
];

export const advancedNavScreens: ScreenDefinition[] = [
  ...everydayScreens,
  ...powerUserScreens,
  accountScreen,
];
