import type { LucideIcon } from "lucide-react";
import {
  Bug,
  Clock,
  Cpu,
  FileAudio,
  HelpCircle,
  Home,
  Key,
  Layers,
  Settings2,
  Sparkles,
} from "lucide-react";

export type ScreenId =
  | "overview"
  | "recordings"
  | "audio"
  | "general"
  | "models"
  | "formatting"
  | "license"
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
    description: "Local models, cloud transcription, and remote VoiceTypr servers.",
  },
  {
    id: "formatting",
    label: "Formatting",
    icon: Sparkles,
    description: "AI cleanup, replacements, dictionary, and snippets.",
  },
  {
    id: "general",
    label: "Settings",
    icon: Settings2,
    description: "Hotkeys, paste behavior, microphones, and app preferences.",
  },
  {
    id: "license",
    label: "Licensing",
    icon: Key,
    description: "Trial and license activation.",
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
