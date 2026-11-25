// The tools the Hub offers. They come in groups (a row of four chips), each with a few sub-tabs, so
// the strip never needs to scroll. The order here is the order of ← / → across the whole Hub.

import type { HubGroup, HubTool } from "./types";
import { serverTool, systemTool } from "./system";
import { pomodoroTool } from "./pomodoro";
import { shelfTool } from "./shelf";
import { tasksTool } from "./tasks";
import { workspacesTool } from "./workspaces";
import { notesTool } from "./notes";
import { clipboardTool } from "./clipboard";
import { calendarTool } from "./calendar";
import { weatherTool } from "./weather";
import { quickTool } from "./quick";
import { todayTool } from "./today";

export const HUB_GROUPS: { id: HubGroup; label: string }[] = [
  { id: "today", label: "Today" },
  { id: "focus", label: "Focus" },
  { id: "system", label: "System" },
  { id: "files", label: "Files" },
];

export const TOOLS: HubTool[] = [
  todayTool, tasksTool, calendarTool, weatherTool,
  pomodoroTool, notesTool, workspacesTool,
  systemTool, serverTool, quickTool,
  clipboardTool, shelfTool,
];

export function toolsOf(group: HubGroup): HubTool[] {
  return TOOLS.filter((t) => t.group === group);
}
