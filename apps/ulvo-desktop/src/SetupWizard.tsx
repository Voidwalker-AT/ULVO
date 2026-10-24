import { useState } from "react";
import { invoke } from "./commands";

export interface SetupStatus {
  stage: string;
  configured: boolean;
  paths: { vault_root: string; source: string };
  plan?: {
    plan_id: string;
    available_bytes: number;
    metadata_estimate_bytes: number;
    external_tools: { name: string; ownership: string; executable_path: string }[];
    warnings: string[];
    discovery_partial: boolean;
    inaccessible_paths: string[];
    request: { discovery_consent: boolean; vault_path: string; workspace_roots: string[]; ecosystems: string[]; enable_background_agent: boolean };
  };
}
const stages = ["Welcome", "Privacy & Permissions", "Read-only Discovery", "Machine Assessment", "Vault Location", "Ecosystem Selection", "Migration Plan", "Review", "Apply", "Verify", "Complete"];
export default function SetupWizard({ initial, onComplete }: { initial: SetupStatus; onComplete: (discovery: boolean) => void }) {
  const [step, setStep] = useState(initial.stage === "review" ? 7 : initial.stage === "applying" || initial.stage === "verify" ? 8 : 0);
  const [vault, setVault] = useState(initial.plan?.request.vault_path ?? initial.paths.vault_root);
  const [roots, setRoots] = useState(initial.plan?.request.workspace_roots.join("\n") ?? "");
  const [consent, setConsent] = useState(initial.plan?.request.discovery_consent ?? false);
  const [agent, setAgent] = useState(initial.plan?.request.enable_background_agent ?? false);
  const [ecosystems, setEcosystems] = useState<string[]>(initial.plan?.request.ecosystems ?? []);
  const [state, setState] = useState(initial);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const request = () => ({ vault_path: vault, workspace_roots: roots.split("\n").map(s => s.trim()).filter(Boolean), ecosystems, discovery_consent: consent, enable_background_agent: agent });
  const prepare = async () => {
    setBusy(true); setError("");
    try { const s = await invoke<SetupStatus>("plan_setup", { request: request() }); setState(s); setStep(step === 2 ? 3 : 7); }
    catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  const apply = async () => {
    if (!state.plan) return;
    setBusy(true); setError("");
    try { const s = await invoke<SetupStatus>("apply_setup", { approvedPlanId: state.plan.plan_id }); setState(s); setStep(9); }
    catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  const cancel = async () => {
    setBusy(true); setError("");
    try { setState(await invoke<SetupStatus>("cancel_setup")); setStep(0); }
    catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  return <main className="setup-wizard">
    <h1>Set up ULVO</h1>
    <p>Step {step + 1} of {stages.length}: {stages[step]}</p>
    <progress max={10} value={step} style={{ width: "100%" }} />
    {step === 0 && <p>Run the control center offline, without installing a language runtime. Existing projects, environments and tools stay where they are.</p>}
    {step === 1 && <section><p>No model, runtime or package downloads are part of setup. Diagnostic operation metadata stays local and has a bounded retention policy. Native output is not automatically collected.</p><label><input type="checkbox" checked={consent} onChange={e => setConsent(e.target.checked)} /> Allow bounded read-only tool discovery. Discovered executables will not run.</label><p>Opt out to configure ULVO without inspecting external tools.</p></section>}
    {step === 2 && <section><p>Discovery checks conventional tool locations. Personal folders are not recursively scanned.</p><button disabled={busy} onClick={prepare}>{consent ? "Run authorized discovery" : "Continue without discovery"}</button></section>}
    {step === 3 && <section><p>{state.plan?.external_tools.length ?? 0} tool records found. Scan partial: {state.plan?.discovery_partial ? "yes" : "no within selected bounds"}. Inaccessible locations: {state.plan?.inaccessible_paths.length ?? 0}. This is a bounded metadata inventory; version and compatibility checks require separate verification.</p><ul>{state.plan?.external_tools.slice(0, 20).map((t, i) => <li key={i}>{t.name}: {t.ownership}</li>)}</ul></section>}
    {step === 4 && <section><label>Vault directory (absolute path)<input style={{ width: "100%" }} value={vault} onChange={e => setVault(e.target.value)} /></label><label>Workspace roots (one absolute path per line, optional)<textarea style={{ width: "100%" }} value={roots} onChange={e => setRoots(e.target.value)} /></label><p>Existing data is never moved. A nonempty directory without ULVO ownership is refused.</p></section>}
    {step === 5 && <section><p>Choose ecosystem preferences. Setup does not install or adopt any tool.</p>{["python", "rust", "node", "go", "ruby", "java", "cpp", "dotnet"].map(e => <label key={e} style={{ display: "block" }}><input type="checkbox" checked={ecosystems.includes(e)} onChange={x => setEcosystems(x.target.checked ? [...ecosystems, e] : ecosystems.filter(v => v !== e))} /> {e}</label>)}<label><input type="checkbox" checked={agent} onChange={e => setAgent(e.target.checked)} /> Enable the optional agent after setup and application restart</label></section>}
    {step === 6 && <section><p>No environments are eligible for removal through first-run consent. Use the existing migration review workflow to verify manifests, editable packages, consumers, ABI, replacement execution and rollback for each candidate.</p><p>This plan retains every external tool and legacy environment.</p></section>}
    {step === 7 && <section><h2>Review plan {state.plan?.plan_id}</h2><p>Vault: {state.plan?.request.vault_path}</p><p>Workspaces: {state.plan?.request.workspace_roots.join(", ") || "None"}. Ecosystems: {state.plan?.request.ecosystems.join(", ") || "None"}. Optional agent: {state.plan?.request.enable_background_agent ? "enabled" : "disabled"}.</p><p>Available: {((state.plan?.available_bytes ?? 0) / 1073741824).toFixed(2)} GiB. Setup metadata reserve: {((state.plan?.metadata_estimate_bytes ?? 0) / 1024).toFixed(0)} KiB. Runtime download size: not selected.</p><p>No downloads, project onboarding, external ownership changes, PATH changes or removals are authorized.</p><ul>{state.plan?.warnings.map((w, i) => <li key={i}>{w}</li>)}</ul></section>}
    {step === 8 && <section><p>Apply exactly the reviewed metadata plan. An interrupted apply can be resumed using this plan ID.</p><button disabled={busy || !state.plan} onClick={apply}>Apply reviewed plan</button></section>}
    {step === 9 && <p>{state.configured ? "Vault ownership and exact persisted preferences verified. Existing project behavior was not changed or certified by this check." : "Verification is incomplete. Resume or cancel the operation."}</p>}
    {step === 10 && <p>Setup complete. Provision or reference tools through a separate reviewed operation. Restart the app to activate new workspace watchers and optional agent preferences.</p>}
    {error && <p role="alert" style={{ color: "#ff7676" }}>{error}</p>}
    <div style={{ display: "flex", gap: 12, marginTop: 24 }}>
      {step < 9 && <button disabled={busy} onClick={cancel}>Cancel setup</button>}
      {step > 0 && step < 8 && <button disabled={busy} onClick={() => setStep(step - 1)}>Back</button>}
      {![2, 6, 8, 10].includes(step) && <button disabled={busy || (step === 9 && !state.configured)} onClick={() => setStep(step + 1)}>Continue</button>}
      {step === 6 && <button disabled={busy} onClick={prepare}>Prepare final review</button>}
      {step === 10 && <button onClick={() => onComplete(state.plan?.request.discovery_consent ?? false)}>Open control center</button>}
    </div>
  </main>;
}
