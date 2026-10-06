// The tools the Hub offers, in the order of its sub-tabs.

import type { HubTool } from "./types";
import { serverTool, systemTool } from "./system";
import { pomodoroTool } from "./pomodoro";

export const TOOLS: HubTool[] = [systemTool, serverTool, pomodoroTool];
