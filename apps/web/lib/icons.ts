/**
 * Single source for icons used across OpenGEO web.
 *
 * Re-exports the subset of `lucide-react` icons required by the design
 * (see `_bmad-output/planning-artifacts/ux-redesign-2026-05-29/project/src/icons.jsx`),
 * mapped 1:1 to the prototype names so component code reads identically.
 *
 * Defaults to `size=14`, `strokeWidth=1.5` — Signal direction expects
 * dense, hairline-weight glyphs. Override per-call when needed:
 *   `<Icon.Activity size={11} strokeWidth={2} />`
 */
import {
  Activity,
  Box,
  Search,
  Bell,
  Settings,
  ChevronRight,
  ChevronDown,
  ChevronUp,
  Plus,
  Check,
  X,
  Play,
  Terminal,
  Cloud,
  Server,
  Globe,
  GitBranch,
  Database,
  Layers,
  BarChart,
  Network,
  AlertTriangle,
  TrendingUp,
  TrendingDown,
  Lock,
  Code,
  Sparkles,
  Bot,
  Copy,
  FileCode,
  ArrowRight,
  ArrowLeft,
  Filter,
  Download,
  ExternalLink,
  RefreshCw,
  Pause,
  Eye,
  Calendar,
  type LucideIcon,
} from "lucide-react";

export type { LucideIcon };

/** Default props shared by every OpenGEO icon. */
export const ICON_DEFAULTS = {
  size: 14,
  strokeWidth: 1.5,
} as const;

/** Prototype-aliased icon registry (matches `shell.jsx` lookups). */
export const Icon = {
  Activity,
  Box,
  Search,
  Bell,
  Settings,
  ChevronRight,
  ChevronDown,
  ChevronUp,
  Plus,
  Check,
  X,
  Play,
  Terminal,
  Cloud,
  Server,
  Globe,
  Branch: GitBranch,
  Database,
  Layers,
  Chart: BarChart,
  Network,
  Alert: AlertTriangle,
  Trend: TrendingUp,
  TrendDown: TrendingDown,
  Lock,
  Code,
  Sparkle: Sparkles,
  Bot,
  Copy,
  Yaml: FileCode,
  ArrowRight,
  ArrowLeft,
  Filter,
  Download,
  ExternalLink,
  Refresh: RefreshCw,
  Pause,
  Eye,
  Calendar,
} as const satisfies Record<string, LucideIcon>;

export type IconName = keyof typeof Icon;
