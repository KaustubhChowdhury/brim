import { describe, expect, it } from "vitest";
import {
  addDays,
  dueLabel,
  formatBytes,
  formatDuration,
  fromLocalInput,
  livePosition,
  monthGrid,
  nextQuarterHour,
  relativeTime,
  startOfDay,
  toLocalInput,
} from "./format";

describe("formatBytes", () => {
  it("formats units", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1023)).toBe("1023 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(25 * 1024 * 1024)).toBe("25 MB");
    expect(formatBytes(-5)).toBe("0 B");
  });
});

describe("formatDuration", () => {
  it("formats minutes and hours", () => {
    expect(formatDuration(5_000)).toBe("0:05");
    expect(formatDuration(187_000)).toBe("3:07");
    expect(formatDuration(3_723_000)).toBe("1:02:03");
    expect(formatDuration(-1)).toBe("0:00");
  });
});

describe("relativeTime", () => {
  const now = new Date(2026, 9, 12, 15, 0).getTime();
  it("covers recent ranges", () => {
    expect(relativeTime(now - 10_000, now)).toBe("just now");
    expect(relativeTime(now - 5 * 60_000, now)).toBe("5 min ago");
    expect(relativeTime(now - 3 * 3_600_000, now)).toBe("3 h ago");
    expect(relativeTime(new Date(2026, 9, 11, 20, 0).getTime(), now)).toBe("Yesterday");
  });
});

describe("local datetime inputs", () => {
  it("round-trips local time", () => {
    const ms = new Date(2026, 0, 31, 9, 5).getTime();
    expect(toLocalInput(ms)).toBe("2026-01-31T09:05");
    expect(fromLocalInput("2026-01-31T09:05")).toBe(ms);
  });
  it("rejects invalid dates", () => {
    expect(fromLocalInput("")).toBeNull();
    expect(fromLocalInput("2026-02-31T10:00")).toBeNull();
    expect(fromLocalInput("garbage")).toBeNull();
  });
});

describe("monthGrid", () => {
  it("starts on Monday and covers the month", () => {
    const grid = monthGrid(2026, 9); // October 2026 starts on a Thursday
    expect(grid).toHaveLength(42);
    expect(new Date(grid[0]!).getDay()).toBe(1);
    expect(grid).toContain(startOfDay(new Date(2026, 9, 1).getTime()));
    expect(grid).toContain(startOfDay(new Date(2026, 9, 31).getTime()));
  });
});

describe("dueLabel", () => {
  const now = new Date(2026, 9, 12, 12, 0).getTime();
  it("flags overdue and today", () => {
    expect(dueLabel(new Date(2026, 9, 12, 18, 0).getTime(), now).today).toBe(true);
    expect(dueLabel(addDays(now, -3), now).overdue).toBe(true);
    expect(dueLabel(addDays(now, 1), now).text.startsWith("Tomorrow")).toBe(true);
  });
});

describe("nextQuarterHour", () => {
  it("rounds to a future quarter hour", () => {
    const now = new Date(2026, 9, 12, 10, 7).getTime();
    expect(new Date(nextQuarterHour(now)).getMinutes()).toBe(15);
    expect(nextQuarterHour(now)).toBeGreaterThan(now);
  });
});

describe("livePosition", () => {
  it("interpolates while playing and clamps", () => {
    expect(livePosition(1000, 10_000, 0, true, 2000)).toBe(3000);
    expect(livePosition(1000, 10_000, 0, false, 2000)).toBe(1000);
    expect(livePosition(9000, 10_000, 0, true, 50_000)).toBe(10_000);
    expect(livePosition(null, 10_000, 0, true, 1)).toBeNull();
  });
});
