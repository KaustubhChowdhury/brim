import {
  Activity,
  Search,
  AppWindow,
  CalendarCheck,
  Power,
  SlidersHorizontal,
  Bookmark,
  Calendar,
  CalendarDays,
  Clipboard,
  ClipboardList,
  Code,
  FileText,
  Headphones,
  Heart,
  Image,
  LayoutDashboard,
  LayoutGrid,
  ListTodo,
  Music,
  Palette,
  Rocket,
  Sparkles,
  SquareCheck,
  Star,
  Terminal,
  Zap,
  type LucideIcon,
} from "lucide-react";
import type { Utility } from "../lib/types";

/** Must match `ALLOWED_ICONS` in src-tauri/src/services/slots.rs. */
export const SLOT_ICONS: Record<string, LucideIcon> = {
  music: Music,
  headphones: Headphones,
  clipboard: Clipboard,
  "clipboard-list": ClipboardList,
  "layout-grid": LayoutGrid,
  rocket: Rocket,
  "check-square": SquareCheck,
  "list-todo": ListTodo,
  calendar: Calendar,
  "calendar-days": CalendarDays,
  code: Code,
  "file-text": FileText,
  image: Image,
  palette: Palette,
  star: Star,
  heart: Heart,
  bolt: Zap,
  bookmark: Bookmark,
  terminal: Terminal,
  sparkles: Sparkles,
};

export const UTILITY_ICON: Record<Utility, LucideIcon> = {
  dashboard: LayoutDashboard,
  music: Music,
  clipboard: Clipboard,
  apps: LayoutGrid,
  tasks: SquareCheck,
  calendar: CalendarDays,
  snippets: Code,
  wallpapers: Image,
  controls: SlidersHorizontal,
  today: CalendarCheck,
  stats: Activity,
  power: Power,
  search: Search,
};

export const UTILITY_LABEL: Record<Utility, string> = {
  dashboard: "Dashboard",
  music: "Music",
  clipboard: "Clipboard",
  apps: "Apps",
  tasks: "Tasks",
  calendar: "Calendar",
  snippets: "Snippets",
  wallpapers: "Wallpaper",
  controls: "Controls",
  today: "Today",
  stats: "Stats",
  power: "Power",
  search: "Search",
};

export function slotIcon(kind: string, icon: string | null): LucideIcon {
  if (icon && SLOT_ICONS[icon]) return SLOT_ICONS[icon]!;
  if (kind in UTILITY_ICON) return UTILITY_ICON[kind as Utility];
  return AppWindow;
}
