// Typed wrappers around Tauri commands. Every call either resolves with data or rejects
// with an `AppError` ({ code, message }) whose message is safe to show to the user.

import { invoke } from "@tauri-apps/api/core";
import type {
  AppEntry,
  AppError,
  CalendarEvent,
  ClipboardStatus,
  ClipPage,
  DockState,
  EventInput,
  ImportResult,
  MediaCommand,
  MediaState,
  NotificationStatus,
  Reminder,
  ReminderInput,
  Settings,
  ShortcutStatus,
  SlotAssignment,
  SlotView,
  Snippet,
  SnippetInput,
  StorageInfo,
  Task,
  TaskInput,
  TaskPatch,
  PowerAction,
  SystemStats,
  Utility,
  VolumeState,
  WeControl,
  WeStatus,
  WeWallpaper,
  Wallpaper,
  WallpaperFit,
} from "./types";

function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args).catch((e: unknown) => {
    throw toAppError(e);
  });
}

export function toAppError(e: unknown): AppError {
  if (e && typeof e === "object" && "message" in e && "code" in e) return e as AppError;
  if (typeof e === "string") return { code: "unknown", message: e };
  if (e instanceof Error) return { code: "unknown", message: e.message };
  return { code: "unknown", message: "Something went wrong." };
}

export const api = {
  dock: {
    ready: () => call<void>("dock_ready"),
    state: () => call<DockState>("dock_get_state"),
    expand: (utility?: Utility) => call<DockState>("dock_expand", { utility: utility ?? null }),
    collapse: (restoreFocus: boolean) => call<void>("dock_collapse", { restoreFocus }),
    peek: (show: boolean) => call<void>("dock_peek", { show }),
    intro: () => call<void>("dock_intro"),
    activityDone: (toIdle: boolean) => call<void>("dock_activity_done", { toIdle }),
    openSettings: (section?: string) => call<void>("dock_open_settings", { section: section ?? null }),
    quit: () => call<void>("app_quit"),
  },
  settings: {
    get: () => call<Settings>("settings_get"),
    update: (patch: Partial<Settings>) => call<Settings>("settings_update", { patch }),
    startupGet: () => call<boolean>("startup_get"),
    startupSet: (enabled: boolean) => call<boolean>("startup_set", { enabled }),
    storageInfo: () => call<StorageInfo>("storage_info"),
    openStorageFolder: () => call<void>("storage_open_folder"),
    deleteAllData: () => call<void>("data_delete_all"),
    finishOnboarding: (choices: { clipboardCapture: boolean; launchAtStartup: boolean }) =>
      call<Settings>("onboarding_finish", { choices }),
    notificationStatus: () => call<NotificationStatus>("notification_status"),
    notificationTest: () => call<void>("notification_test"),
    shortcutStatus: () => call<ShortcutStatus>("shortcut_status"),
  },
  slots: {
    list: () => call<SlotView[]>("slots_list"),
    set: (slot: number, assignment: SlotAssignment) => call<SlotView[]>("slots_set", { slot, assignment }),
    move: (from: number, to: number) => call<SlotView[]>("slots_move", { from, to }),
    reset: () => call<SlotView[]>("slots_reset"),
  },
  clipboard: {
    list: (query: string, filter: "all" | "pinned", limit: number, offset: number) =>
      call<ClipPage>("clipboard_list", { query, filter, limit, offset }),
    copy: (id: number) => call<void>("clipboard_copy", { id }),
    get: (id: number) => call<string>("clipboard_get", { id }),
    pin: (id: number, pinned: boolean) => call<void>("clipboard_pin", { id, pinned }),
    remove: (id: number) => call<void>("clipboard_delete", { id }),
    clear: (includePinned: boolean) => call<number>("clipboard_clear", { includePinned }),
    status: () => call<ClipboardStatus>("clipboard_status"),
    pause: (minutes: number | null) => call<Settings>("clipboard_pause", { minutes }),
    resume: () => call<Settings>("clipboard_resume"),
  },
  apps: {
    list: () => call<AppEntry[]>("apps_list"),
    scanning: () => call<boolean>("apps_scanning"),
    launch: (id: string) => call<void>("apps_launch", { id }),
    pin: (id: string, pinned: boolean) => call<void>("apps_pin", { id, pinned }),
    reorder: (ids: string[]) => call<void>("apps_reorder", { ids }),
    exclude: (id: string, excluded: boolean) => call<void>("apps_exclude", { id, excluded }),
    addCustom: () => call<string | null>("apps_add_custom"),
    remove: (id: string) => call<void>("apps_remove", { id }),
    clearRecent: () => call<void>("apps_clear_recent"),
    reset: () => call<void>("apps_reset"),
    refresh: () => call<void>("apps_refresh"),
  },
  media: {
    state: () => call<MediaState>("media_state"),
    command: (command: MediaCommand) => call<void>("media_command", { command }),
    seek: (positionMs: number) => call<void>("media_seek", { positionMs: Math.max(0, Math.round(positionMs)) }),
    select: (sessionId: string | null) => call<void>("media_select", { sessionId }),
  },
  volume: {
    state: () => call<VolumeState>("volume_state"),
    set: (level: number) => call<void>("volume_set", { level: Math.min(1, Math.max(0, level)) }),
    mute: (muted: boolean) => call<void>("volume_mute", { muted }),
  },
  search: {
    web: (engine: "youtube" | "google", query: string) => call<void>("web_search", { engine, query }),
  },
  wallpaperEngine: {
    status: () => call<WeStatus>("we_status"),
    list: () => call<WeWallpaper[]>("we_list"),
    preview: (id: string) => call<string | null>("we_preview", { id }),
    apply: (id: string) => call<void>("we_apply", { id }),
    control: (control: WeControl) => call<void>("we_control", { control }),
  },
  widget: {
    setVisible: (visible: boolean) => call<void>("widget_set_visible", { visible }),
  },
  system: {
    stats: () => call<SystemStats>("system_stats"),
    power: (action: PowerAction) => call<void>("power_action", { action }),
  },
  tasks: {
    list: () => call<Task[]>("tasks_list"),
    create: (input: TaskInput) => call<Task>("tasks_create", { input }),
    update: (id: number, patch: TaskPatch) => call<Task>("tasks_update", { id, patch }),
    remove: (id: number) => call<void>("tasks_delete", { id }),
    clearCompleted: () => call<number>("tasks_clear_completed"),
  },
  reminders: {
    list: () => call<Reminder[]>("reminders_list"),
    create: (input: ReminderInput) => call<Reminder>("reminders_create", { input }),
    update: (id: number, input: ReminderInput) => call<Reminder>("reminders_update", { id, input }),
    dismiss: (id: number) => call<Reminder>("reminders_dismiss", { id }),
    remove: (id: number) => call<void>("reminders_delete", { id }),
    clearDismissed: () => call<number>("reminders_clear_dismissed"),
  },
  calendar: {
    list: (from: number, to: number) => call<CalendarEvent[]>("calendar_list", { from, to }),
    create: (input: EventInput) => call<CalendarEvent>("calendar_create", { input }),
    update: (id: number, input: EventInput) => call<CalendarEvent>("calendar_update", { id, input }),
    remove: (id: number) => call<void>("calendar_delete", { id }),
  },
  snippets: {
    list: () => call<Snippet[]>("snippets_list"),
    create: (input: SnippetInput) => call<Snippet>("snippets_create", { input }),
    update: (id: number, input: SnippetInput) => call<Snippet>("snippets_update", { id, input }),
    remove: (id: number) => call<void>("snippets_delete", { id }),
    copy: (id: number) => call<void>("snippets_copy", { id }),
  },
  wallpapers: {
    list: () => call<Wallpaper[]>("wallpapers_list"),
    import: () => call<ImportResult | null>("wallpapers_import"),
    preview: (id: number) => call<string>("wallpapers_preview", { id }),
    apply: (id: number, fit: WallpaperFit | null) => call<void>("wallpapers_apply", { id, fit }),
    favourite: (id: number, favourite: boolean) => call<void>("wallpapers_favourite", { id, favourite }),
    rename: (id: number, title: string) => call<void>("wallpapers_rename", { id, title }),
    remove: (id: number) => call<void>("wallpapers_remove", { id }),
  },
};
