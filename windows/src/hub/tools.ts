// The tools the Hub offers, in the order of its sub-tabs.

import type { HubTool } from "./types";
import { serverTool, systemTool } from "./system";
import { pomodoroTool } from "./pomodoro";
import { shelfTool } from "./shelf";
import { notesTool } from "./notes";
import { calendarTool } from "./calendar";

export const TOOLS: HubTool[] = [systemTool, serverTool, pomodoroTool, notesTool, calendarTool, shelfTool];
