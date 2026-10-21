// The real glasses: the Even Hub SDK behind the app's small `Glasses` interface. Everything here needs the Even
// phone app (or the simulator); the logic worth testing lives in the other files.

import {
  AudioInputSource,
  CreateStartUpPageContainer,
  OsEventTypeList,
  TextContainerProperty,
  TextContainerUpgrade,
  waitForEvenAppBridge,
  type EvenAppBridge,
} from "@evenrealities/even_hub_sdk";
import { EV } from "./events.ts";
import type { Glasses } from "./app.ts";

// events.ts repeats the SDK's numbers so it can be tested without the SDK; this keeps the two in step.
const _inStep: Record<keyof typeof EV, OsEventTypeList> = {
  CLICK: OsEventTypeList.CLICK_EVENT,
  SCROLL_TOP: OsEventTypeList.SCROLL_TOP_EVENT,
  SCROLL_BOTTOM: OsEventTypeList.SCROLL_BOTTOM_EVENT,
  DOUBLE_CLICK: OsEventTypeList.DOUBLE_CLICK_EVENT,
  FOREGROUND_ENTER: OsEventTypeList.FOREGROUND_ENTER_EVENT,
  FOREGROUND_EXIT: OsEventTypeList.FOREGROUND_EXIT_EVENT,
  ABNORMAL_EXIT: OsEventTypeList.ABNORMAL_EXIT_EVENT,
  SYSTEM_EXIT: OsEventTypeList.SYSTEM_EXIT_EVENT,
  LONG_PRESS: OsEventTypeList.LONG_PRESS_EVENT,
  LONG_PRESS_RELEASE: OsEventTypeList.LONG_PRESS_RELEASE_EVENT,
};
for (const k of Object.keys(EV) as (keyof typeof EV)[]) {
  if (EV[k] !== _inStep[k]) console.warn(`events.ts out of step with the SDK: ${k}`);
}

const ID = 1;
const NAME = "jinx";

export interface Connected { bridge: EvenAppBridge; glasses: Glasses }

export async function connect(): Promise<Connected> {
  const bridge = await waitForEvenAppBridge();
  const main = new TextContainerProperty({
    xPosition: 0, yPosition: 0, width: 576, height: 288,
    borderWidth: 0, borderColor: 5, paddingLength: 4,
    containerID: ID, containerName: NAME, content: "Jinx", isEventCapture: 1,
  });
  const created = await bridge.createStartUpPageContainer(
    new CreateStartUpPageContainer({ containerTotalNum: 1, textObject: [main] }),
  );
  if (created !== 0) console.error("createStartUpPageContainer failed:", created);
  const glasses: Glasses = {
    async show(text) {
      await bridge.textContainerUpgrade(new TextContainerUpgrade({ containerID: ID, containerName: NAME, content: text }));
    },
    async mic(on) {
      await bridge.audioControl(on, on ? AudioInputSource.Glasses : undefined);
    },
    exit() {
      bridge.shutDownPageContainer(1);
    },
  };
  return { bridge, glasses };
}
