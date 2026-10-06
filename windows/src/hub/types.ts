// The Hub: one header tab that opens a view with a sub-tab per tool (System, Server,
// Pomodoro, Notes, Calendar, Weather, Shelf, Quick). A tool is a small module; the Hub
// builds its panel once, and tells it to `start` reading its source only while the panel
// is on screen and to `stop` the moment it is not — the island costs nothing while shut.

export interface HubHost {
  el: HTMLElement;
  /** Called on every redraw while the panel is shown. */
  sync(): void;
  /** The panel has a text field: give it the keyboard. */
  focus?(): void;
  /** The panel came on screen: begin polling whatever it shows. */
  start?(): void;
  /** It went off screen (another tab, another view, the island closed): stop all of it. */
  stop?(): void;
}

export interface HubTool {
  id: string;
  label: string;
  /** Does this tool have a text field (and so need the island to take the keyboard)? */
  typing?: boolean;
  build(): HubHost;
}
