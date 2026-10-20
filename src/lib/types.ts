// TypeScript mirror of the Rust command/event contract (src-tauri/src/**).
// Keep field names in sync with the `#[serde(rename_all = "camelCase")]` structs.

export type Utility =
  | "dashboard"
  | "music"
  | "clipboard"
  | "apps"
  | "tasks"
  | "calendar"
  | "snippets"
  | "wallpapers"
  | "controls"
  | "today"
  | "stats"
  | "power"
  | "search";
/** Utilities offered in the compact layout's "All utilities" grid. */
export const UTILITIES: Utility[] = ["music", "clipboard", "apps", "search", "tasks", "calendar", "snippets", "wallpapers", "stats", "power"];
/** Tabs of the wide layout, in order. */
export const WIDE_TABS: Utility[] = ["dashboard", "controls", "clipboard", "apps"];
/** Everything else in the wide layout lives behind the header's grid button. */
export const WIDE_MORE: Utility[] = ["today", "search", "stats", "wallpapers", "snippets", "power"];
export type PanelStyle = "compact" | "wide";

export type Theme = "dark" | "light" | "aqua" | "midnight" | "system";
export type AnimationLevel = "full" | "reduced" | "none";
export type CompactVisibility = "vanish" | "line" | "always" | "fade" | "hidden";
export type IdleForm = "vanish" | "line";
export type CaptureState = "on" | "paused" | "off";
export type WallpaperFit = "fill" | "fit" | "stretch" | "center" | "tile" | "span";
export type TrayClickAction = "toggle" | "settings";

export interface Settings {
  theme: Theme;
  accentColor: string;
  surfaceOpacity: number;
  cornerRadius: number;
  dockScale: number;
  panelStyle: PanelStyle;
  animationLevel: AnimationLevel;
  alwaysOnTop: boolean;
  hideOnFullscreen: boolean;
  collapseOnBlur: boolean;
  expandOnScroll: boolean;
  compactVisibility: CompactVisibility;
  shortcut: string;
  shortcutCustomized: boolean;
  collapseAfterCopy: boolean;
  collapseAfterLaunch: boolean;
  trayClickAction: TrayClickAction;
  defaultsVersion: number;
  restoreLastUtility: boolean;
  lastUtility: Utility;
  onboardingCompleted: boolean;
  clipboardCapture: CaptureState;
  clipboardPausedUntil: number | null;
  clipboardRetentionDays: number;
  clipboardMaxItems: number;
  clipboardMaxEntryKb: number;
  clipboardSkipSecrets: boolean;
  clipboardIgnoredApps: string[];
  showNowPlaying: boolean;
  showVolumeHud: boolean;
  handleVolumeKeys: boolean;
  musicWidget: boolean;
  musicWidgetStyle: "card" | "compact";
  musicWidgetOnTop: boolean;
  musicWidgetHideIdle: boolean;
  trackRecentApps: boolean;
  notificationsEnabled: boolean;
  wallpaperFit: WallpaperFit;
}

export interface AppError {
  code: string;
  message: string;
  detail?: string;
}

export type Direction = "down" | "up";
export type ActivityKind = "nowPlaying" | "volume";

export interface RadioTile {
  on: boolean;
  connected: boolean;
  /** The Wi-Fi network name, when connected and Windows shares it. */
  network: string | null;
}

export interface AudioOutput {
  id: string;
  name: string;
  isDefault: boolean;
}

export interface QuickState {
  /** null when the PC has no such radio. */
  wifi: RadioTile | null;
  bluetooth: RadioTile | null;
  /** null when Night light's state can't be read. */
  nightLight: boolean | null;
  /** false until Night light has been turned on once in Windows. */
  nightLightSwitchable: boolean;
  darkMode: boolean;
  outputs: AudioOutput[];
}

export type QuickSetting = "wifi" | "bluetooth" | "nightLight" | "darkMode";
export type QuickPage = "wifi" | "bluetooth" | "sound" | "nightLight" | "colors";

export interface SystemInfo {
  userName: string;
  osName: string;
  osVersion: string;
  computerName: string;
}

export interface VolumeState {
  available: boolean;
  /** 0 – 1 */
  level: number;
  muted: boolean;
}

export interface DockState {
  expanded: boolean;
  utility: Utility;
  direction: Direction;
  pillOffsetX: number;
  pillOffsetY: number;
  hiddenForFullscreen: boolean;
  /** Collapsed to its idle form (vanished or a line). */
  idle: boolean;
  /** A live activity pill is showing. */
  activity: ActivityKind | null;
}

export interface SlotView {
  slotNumber: number;
  kind: Utility | "app" | string;
  target: string | null;
  label: string;
  customLabel: string | null;
  icon: string | null;
  status: "ok" | "missing" | "invalid";
  statusMessage: string | null;
}

export interface SlotAssignment {
  kind: string;
  target: string | null;
  label: string | null;
  icon: string | null;
}

export interface ClipItem {
  id: number;
  preview: string;
  truncated: boolean;
  contentType: string;
  createdAt: number;
  lastUsedAt: number;
  pinned: boolean;
  sizeBytes: number;
  copyCount: number;
  lineCount: number;
}

export interface ClipPage {
  items: ClipItem[];
  total: number;
}

export interface ClipStats {
  count: number;
  pinned: number;
  totalBytes: number;
}

export interface ClipNotice {
  kind: "tooLarge" | "secret" | "private" | "ignoredApp";
  sizeBytes: number | null;
  app: string | null;
  at: number;
}

export interface ClipboardStatus {
  capture: CaptureState;
  pausedUntil: number | null;
  retentionDays: number;
  maxItems: number;
  stats: ClipStats;
  lastNotice: ClipNotice | null;
}

export interface AppEntry {
  id: string;
  name: string;
  source: "shell" | "custom";
  iconKey: string | null;
  pinned: boolean;
  pinOrder: number | null;
  excluded: boolean;
  available: boolean;
  path: string | null;
  lastUsedAt: number | null;
  launchCount: number;
  icon: string | null;
}

export type PlaybackStatus = "playing" | "paused" | "stopped" | "changing" | "opened" | "closed";

export interface MediaSessionInfo {
  id: string;
  appName: string;
  status: PlaybackStatus;
}

export interface NowPlaying {
  sessionId: string;
  appName: string;
  title: string;
  artist: string;
  album: string;
  artwork: string | null;
  status: PlaybackStatus;
  canPlayPause: boolean;
  canNext: boolean;
  canPrevious: boolean;
  canSeek: boolean;
  positionMs: number | null;
  durationMs: number | null;
  positionUpdatedAt: number | null;
}

export interface MediaState {
  available: boolean;
  sessions: MediaSessionInfo[];
  active: NowPlaying | null;
  manualSelection: boolean;
}

export type MediaCommand = "playPause" | "next" | "previous";

export interface Task {
  id: number;
  title: string;
  notes: string | null;
  completed: boolean;
  completedAt: number | null;
  priority: number;
  groupName: string | null;
  dueAt: number | null;
  createdAt: number;
  updatedAt: number;
}

export interface TaskInput {
  title: string;
  notes?: string | null;
  priority?: number;
  groupName?: string | null;
  dueAt?: number | null;
}

export interface TaskPatch {
  title?: string;
  notes?: string | null;
  completed?: boolean;
  priority?: number;
  groupName?: string | null;
  dueAt?: number | null;
}

export type ReminderState = "scheduled" | "notified" | "missed" | "dismissed";

export interface Reminder {
  id: number;
  title: string;
  scheduledAt: number;
  notificationState: ReminderState;
  notifiedAt: number | null;
  dismissedAt: number | null;
  launchAppId: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface ReminderInput {
  title: string;
  scheduledAt: number;
  launchAppId: string | null;
}

export interface CalendarEvent {
  id: number;
  title: string;
  startAt: number;
  endAt: number;
  allDay: boolean;
  description: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface EventInput {
  title: string;
  startAt: number;
  endAt: number;
  allDay: boolean;
  description: string | null;
}

export interface Snippet {
  id: number;
  title: string;
  content: string;
  category: string | null;
  createdAt: number;
  updatedAt: number;
  lastUsedAt: number | null;
}

export interface SnippetInput {
  title: string;
  content: string;
  category: string | null;
}

export interface Wallpaper {
  id: number;
  title: string;
  favourite: boolean;
  width: number | null;
  height: number | null;
  createdAt: number;
  missing: boolean;
  thumbnail: string | null;
}

export interface ImportResult {
  imported: number;
  duplicates: number;
  failed: string[];
}

export interface StorageInfo {
  dataDir: string;
  databaseBytes: number;
  wallpaperBytes: number;
  cacheBytes: number;
  clipboard: ClipStats;
}

export interface NotificationStatus {
  state: "enabled" | "disabledForApp" | "disabledForUser" | "disabledByPolicy" | "unknown";
}

export interface ShortcutStatus {
  shortcut: string;
  registered: boolean;
}

export interface Battery {
  percent: number;
  charging: boolean;
  pluggedIn: boolean;
}

export interface SystemStats {
  cpuPercent: number | null;
  memoryUsed: number;
  memoryTotal: number;
  diskUsed: number;
  diskTotal: number;
  diskLabel: string;
  battery: Battery | null;
  uptimeSecs: number;
}

export type PowerAction = "lock" | "sleep" | "restart" | "shutdown" | "signout";

export interface WeWallpaper {
  id: string;
  title: string;
  kind: string;
  source: "workshop" | "mine" | "builtin";
}

export interface WeStatus {
  installed: boolean;
  running: boolean;
}

export type WeControl = "play" | "pause" | "mute" | "unmute";
