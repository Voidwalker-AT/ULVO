import { useState } from "react";
import { invoke } from "./commands";
interface Candidate { project: string; eligible_for_connector: boolean; disposition: string; rollback: string; removal_authorized: boolean }
interface Plan { plan_id: string; candidates: Candidate[]; applied: string[] }
export default function MigrationReview() {
  const [projects, setProjects] = useState(""); const [workspace, setWorkspace] = useState("");
  const [plan, setPlan] = useState<Plan | null>(null); const [selected, setSelected] = useState<string[]>([]);
  const [error, setError] = useState(""); const [busy, setBusy] = useState(false);
  const review = async (all: boolean) => {
    setBusy(true); setError("");
    try { setPlan(await invoke<Plan>("review_migration_batch", { projects: all ? [] : projects.split("\n").map(s => s.trim()).filter(Boolean), workspace: all ? workspace : null })); setSelected([]); }
    catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  const apply = async () => {
    if (!plan || !selected.length) return; setBusy(true); setError("");
    try { setPlan(await invoke<Plan>("apply_migration_batch", { approvedPlanId: plan.plan_id, selected })); }
    catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  return <section><h2>Migration review</h2><p>This first batch creates thin connectors only. Legacy environments, native manifests, lockfiles and external tools are retained. Actual replacement execution, compatibility, consumers and rollback must be verified separately before any retirement.</p>
    <label>One or more project directories (one absolute path per line)<textarea value={projects} onChange={e => setProjects(e.target.value)} style={{ width: "100%" }} /></label>
    <button disabled={busy} onClick={() => review(false)}>Review listed candidates</button>
    <label>Workspace directory for bounded candidate review<input value={workspace} onChange={e => setWorkspace(e.target.value)} /></label>
    <button disabled={busy || !workspace} onClick={() => review(true)}>Review all candidates in this workspace</button>
    {plan && <section><h3>Reviewed plan {plan.plan_id}</h3><button onClick={() => setSelected(plan.candidates.filter(c => c.eligible_for_connector).map(c => c.project))}>Select all eligible connectors</button>
      {plan.candidates.map(c => <article key={c.project} style={{ padding: 12, borderBottom: "1px solid #555" }}><label><input type="checkbox" disabled={!c.eligible_for_connector || busy} checked={selected.includes(c.project)} onChange={e => setSelected(e.target.checked ? [...selected, c.project] : selected.filter(p => p !== c.project))} />{c.project}</label><p>{c.disposition}</p><p>{c.rollback}</p><strong>{plan.applied.includes(c.project) ? "Connector applied; environment retained" : c.eligible_for_connector ? "Eligible for connector only" : "Blocked"}. Removal authorized: no.</strong></article>)}
      <button disabled={busy || !selected.length} onClick={apply}>Apply reviewed connectors to {selected.length} selected candidates</button></section>}
    {error && <p role="alert">{error}</p>}
  </section>;
}
