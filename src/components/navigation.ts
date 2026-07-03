import type { LucideIcon } from "lucide-react";
import {
  Bug,
  Clock,
  Cpu,
  FileAudio,
  HelpCircle,
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
  | "help";

export type SidebarActionId = "report-bug";

export interface ScreenDefinition {
  id: ScreenId;
  label: string;
  icon: LucideIcon;
  description: string;
}

export interface SidebarActionDefinition {
  id: SidebarActionId;
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
    label: "Transcription",
    icon: Cpu,
    description: "Local models, cloud transcription, and remote Voicetypr servers.",
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
    label: "Licensing",
    icon: Key,
    description: "Trial and license activation.",
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
    id: "help",
    label: "Help",
    icon: HelpCircle,
    description: "Troubleshooting, support, and bug reporting.",
  },
];

export const sidebarActions: SidebarActionDefinition[] = [
  {
    id: "report-bug",
    label: "Report Bug",
    icon: Bug,
    description: "Send a bug report with diagnostic logs.",
  },
];

export const screens = [...primaryScreens, ...secondaryScreens] as const;

export const isScreenId = (value: string): value is ScreenId =>
  screens.some((screen) => screen.id === value);

export interface NavGroup {
  label: string;
  screens: ScreenDefinition[];
}

const screenById = (id: ScreenId): ScreenDefinition =>
  screens.find((screen) => screen.id === id) as ScreenDefinition;

// Grouped sidebar layout (Claude dashboard design): Workspace / Configure / Account.
export const navGroups: NavGroup[] = [
  {
    label: "Workspace",
    screens: [screenById("overview"), screenById("recordings"), screenById("audio")],
  },
  {
    label: "Configure",
    screens: [
      screenById("general"),
      screenById("shortcuts"),
      screenById("models"),
      screenById("network"),
      screenById("formatting"),
      screenById("agent"),
    ],
  },
  {
    label: "Account",
    screens: [screenById("license"), screenById("advanced")],
  },
];

export const footerScreens: ScreenDefinition[] = [screenById("help")];
