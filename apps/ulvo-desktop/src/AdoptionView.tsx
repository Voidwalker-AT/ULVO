import { useEffect, useRef, useState } from "react";
import type { AdoptionPlanReport } from "./types";

export type AdoptionState = { allowed: boolean | null; loading: boolean; report: AdoptionPlanReport | null; error: string };
export const initialAdoption: AdoptionState = { allowed: null, loading: true, report: null, error: "" };
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

// Late replies cannot republish a result after permission changes or unmount.
export function adoptionController(invoke: Invoke, publish: (s: AdoptionState) => void) {
  let generation = 0;
  let state = initialAdoption;
  const emit = (s: AdoptionState) => { state = s; publish(s); };
  const bounded = async <T,>(promise: Promise<T>): Promise<T> => {
    let timer: ReturnType<typeof setTimeout>;
    try { return await Promise.race([promise, new Promise<T>((_, reject) => { timer = setTimeout(() => reject(new Error("Discovery timed out. Retry or leave discovery disabled.")), 10000); })]); }
    finally { clearTimeout(timer!); }
  };
  const scan = async (id: number) => {
    try {
      const report = await bounded(invoke<AdoptionPlanReport>("get_adoption_report"));
      if (id === generation) emit({ allowed: true, loading: false, report, error: "" });
    } catch (error) { if (id === generation) emit({ ...state, loading: false, error: String(error) }); }
  };
  return {
    async load() {
      const id = ++generation;
      emit({ ...state, loading: true, error: "" });
      try {
        const allowed = await bounded(invoke<boolean>("get_discovery_permission"));
        if (id !== generation) return;
        emit({ allowed, loading: allowed, report: null, error: "" });
        if (allowed) await scan(id);
      } catch (error) { if (id === generation) emit({ ...state, loading: false, error: String(error) }); }
    },
    async permission(allowed: boolean) {
      const previous = state;
      const id = ++generation;
      emit({ ...state, loading: true, error: "" });
      try {
        await bounded(invoke<void>("set_discovery_permission", { allowed }));
        if (id !== generation) return;
        emit({ allowed, loading: allowed, report: null, error: "" });
        if (allowed) await scan(id);
      } catch (error) { if (id === generation) emit({ ...previous, loading: false, error: `Permission was not changed: ${error}` }); }
    },
    dispose() { generation++; },
  };
}

export function AdoptionContent({ state, permission, retry, installed, adopt }: {
  state: AdoptionState; permission: (allowed: boolean) => void; retry: () => void;
  installed: () => void; adopt: (name: string, path: string, ecosystem: string) => void;
}) {
  return <section className="card" aria-label="Toolchain adoption">
    <h2>Toolchain discovery and adoption</h2>
    <p>Registered tools remain available in Installed. Discovery does not install, move or remove tools.</p>
    <button className="btn" onClick={installed}>View registered tools</button>
    {state.error && <p role="alert">{state.error} Check local diagnostics and retry.</p>}
    {state.loading ? <p role="status">Loading discovery state…</p> : <>
      {state.allowed === false && <p>Discovery is disabled by your privacy choice. Allow bounded read-only metadata discovery to review external tools. Discovered executables will not run.</p>}
      {state.allowed === null ? <button className="btn" onClick={retry}>Retry permission check</button> :
        <button className="btn" onClick={() => permission(!state.allowed)}>{state.allowed ? "Disable discovery" : "Allow metadata discovery"}</button>}
      {state.allowed && <button className="btn" onClick={retry}>Refresh discovery</button>}
      {state.allowed && state.report && (state.report.installations.length === 0 ? <p>No tools found within the discovery bounds. Registered tools are still available in Installed.</p> :
        <table className="data-table"><thead><tr><th>Tool</th><th>Path</th><th>Classification</th><th>Review</th></tr></thead><tbody>
          {state.report.installations.map((tool, i) => <tr key={i}><td>{tool.name}</td><td>{tool.path}</td><td>{tool.classification}</td><td>{tool.explanation}
            {tool.classification === "adoptable" && <button className="btn" onClick={() => adopt(tool.name, tool.path, typeof tool.ecosystem === "string" ? tool.ecosystem : "system")}>Adopt</button>}
          </td></tr>)}
        </tbody></table>)}
    </>}
  </section>;
}

export default function AdoptionView({ invoke, installed, adopt }: { invoke: Invoke; installed: () => void; adopt: (name: string, path: string, ecosystem: string) => void }) {
  const [state, setState] = useState(initialAdoption);
  const controller = useRef<ReturnType<typeof adoptionController> | null>(null);
  useEffect(() => {
    const current = adoptionController(invoke, setState); controller.current = current;
    void current.load();
    return () => current.dispose();
  }, [invoke]);
  return <AdoptionContent state={state} permission={value => void controller.current?.permission(value)} retry={() => void controller.current?.load()} installed={installed} adopt={adopt} />;
}
