// The tools the Hub offers, in the order of its sub-tabs.

import type { HubTool } from "./types";
import { serverTool, systemTool } from "./system";
import { pomodoroTool } from "./pomodoro";
import { shelfTool } from "./shelf";
import { tasksTool } from "./tasks";
import { notesTool } from "./notes";
import { calendarTool } from "./calendar";
import { weatherTool } from "./weather";
import { quickTool } from "./quick";

export const TOOLS: HubTool[] = [systemTool, serverTool, pomodoroTool, tasksTool, notesTool, calendarTool, weatherTool, quickTool, shelfTool];
