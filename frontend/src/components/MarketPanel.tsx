import { useEffect, useState } from "react";
import { api } from "../api";

export function MarketPanel() {
  const [installed, setInstalled] = useState<any[]>([]);
  const [source, setSource] = useState("file:./examples/packages/cool-loop");
  const [inspectResult, setInspectResult] = useState<any | null>(null);
  const [msg, setMsg] = useState("");
  const [err, setErr] = useState("");

  const refresh = () => api.marketInstalled().then(setInstalled);
  useEffect(() => { refresh(); }, []);

  const inspect = async () => {
    setErr(""); setMsg(""); setInspectResult(null);
    try { setInspectResult(await api.marketInspect(source)); }
    catch (e) { setErr(String(e)); }
  };

  const install = async (consent = false) => {
    setErr(""); setMsg("");
    try {
      const info = await api.marketInstall(source, consent);
      setMsg(`Installed ${info.name}@${info.version} (${info.wired})`);
      refresh();
    } catch (e: any) {
      const m = String(e);
      if (m.includes("needs_consent") || m.includes("capabilities")) {
        if (confirm("This package requests capabilities. Install anyway?")) {
          install(true);
          return;
        }
      }
      setErr(m);
    }
  };

  return (
    <div className="panel">
      <h2>Community Market</h2>
      <p className="muted">
        Decentralized: install algorithms or datasets from a git repo, URL tarball, or local path.
        Integrity is verified; declared capabilities require consent.
      </p>
      <label>Source spec</label>
      <input value={source} onChange={(e) => setSource(e.target.value)}
        placeholder="git+https://host/repo.git#tag | https://host/pkg.tar.gz | file:./path" />
      <div style={{ marginTop: 8, display: "flex", gap: 8 }}>
        <button className="ghost" onClick={inspect}>Inspect</button>
        <button className="primary" onClick={() => install(false)}>Install</button>
      </div>
      {msg && <div className="muted" style={{ marginTop: 8 }}>{msg}</div>}
      {err && <div className="err">{err}</div>}

      {inspectResult && (
        <div style={{ marginTop: 12 }}>
          <h3>Manifest</h3>
          <div className="code">{JSON.stringify(inspectResult.manifest, null, 2)}</div>
          {inspectResult.problems?.length > 0 && (
            <div className="err">problems: {inspectResult.problems.join("; ")}</div>
          )}
        </div>
      )}

      <h3>Installed packages</h3>
      <table>
        <thead><tr><th>name</th><th>version</th><th>type</th><th>category</th><th></th></tr></thead>
        <tbody>
          {installed.map((p) => (
            <tr key={p.name}>
              <td>{p.name}</td><td>{p.version}</td><td>{p.type}</td><td>{p.category}</td>
              <td><button className="ghost" onClick={() => api.marketUninstall(p.name).then(refresh)}>remove</button></td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
