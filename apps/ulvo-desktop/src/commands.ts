import { invoke as nativeInvoke, InvokeArgs } from "@tauri-apps/api/core";

// Record operation names and outcomes, never invocation arguments or inventories.
const audited = new Set([
  "plan_setup", "apply_setup", "cancel_setup", "review_migration_batch", "apply_migration_batch",
  "plan_provision", "execute_provision", "recover_staging_transactions", "prune_cache", "gc_cache",
  "get_adoption_report", "adopt_tool", "reclaim_environments", "save_user_preferences",
  "trigger_agent_reconciliation", "scan_workspace_projects", "register_project", "onboard_project",
  "start_agent", "stop_agent", "get_gpu_telemetry",
]);
export async function invoke<T>(command: string, args?: InvokeArgs): Promise<T> {
  const record = async (classification: string, message: string) => {
    if (audited.has(command)) {
      try { await nativeInvoke("record_gui_operation", { command, classification, message }); }
      catch { console.warn("ULVO local diagnostic recording unavailable"); }
    }
  };
  try {
    const result = await nativeInvoke<T>(command, args);
    await record("SUCCESS", "Operation completed");
    return result;
  } catch (error) {
    await record("FAILURE", String(error));
    throw error;
  }
}
