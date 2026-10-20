import assert from "node:assert/strict";
import type { ProductBridge } from "../../main/session-contract.ts";

const productBridgeCapabilities = [
  "startAuth", "submitAuth", "nextAuthEvent", "cancelAuth", "logoutAuth",
  "getSessions", "switchSession", "subscribeSessions", "nextSessionEvent", "unsubscribeSessions",
  "startMessage", "continueMessage", "getActiveMessage", "subscribeMessageEvents", "nextMessageEvent",
  "unsubscribeMessageEvents", "cancelMessage", "getInterruptionState", "respondPermission",
  "respondUserQuestion", "cancelUserQuestion", "getSettings", "updateSetting", "resetSetting",
  "listThemes", "getTheme", "saveTheme", "autocompleteInput",
] as const;

const productBridgeEventCapabilities = ["onModelsChanged", "onRuntimeInitializationError"] as const;

export function assertProductBridgeCapabilityKeys(keys: Iterable<string>, options: { events: boolean }): void {
  const approved = options.events
    ? [...productBridgeCapabilities, ...productBridgeEventCapabilities]
    : [...productBridgeCapabilities];
  assert.deepEqual(
    [...keys].sort(),
    [...approved].sort(),
    "preload bridge must expose exactly the approved product capabilities",
  );
}

export function assertProductBridgeCapabilities(bridge: ProductBridge, options: { events: boolean }): void {
  assertProductBridgeCapabilityKeys(Object.keys(bridge), options);
}
