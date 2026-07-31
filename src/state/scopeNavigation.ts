import type { Scope } from "@/shared";
import { useManagerStore } from "./manager";

interface SuiteManagerSnapshot {
  data: object | null;
  readOnly: boolean;
  refresh: () => Promise<void>;
  setScope: (scope: Scope) => void;
}

type SuiteManagerAccess = () => SuiteManagerSnapshot;

export async function enterSuiteManagerScope(
  access: SuiteManagerAccess = useManagerStore.getState,
): Promise<boolean> {
  let manager = access();
  if (!manager.data || manager.readOnly) {
    await manager.refresh();
    manager = access();
  }
  if (!manager.data || manager.readOnly) return false;
  manager.setScope("suite");
  return true;
}

export async function addWorkspaceAndEnterScope(
  pick: () => Promise<boolean>,
  setScope: (scope: Scope) => void,
): Promise<boolean> {
  if (!(await pick())) return false;
  setScope("workspace");
  return true;
}
