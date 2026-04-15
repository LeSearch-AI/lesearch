import { useEffect, useState } from "react";
import type { DaemonClient } from "../lib/ws";

interface Agent {
  id: string;
  provider: string;
  status: string;
  cwd: string;
  session_id: string;
}

interface Props {
  client: DaemonClient | null;
  onSelect: (id: string) => void;
}

export function AgentList({ client, onSelect }: Props) {
  const [agents, setAgents] = useState<Agent[]>([]);
  const [error, setError] = useState<string | null>(null);

  const refresh = async () => {
    if (!client?.connected) return;
    try {
      const result = (await client.call("agent.list")) as Agent[];
      setAgents(result ?? []);
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  useEffect(() => {
    refresh();
    const interval = setInterval(refresh, 3000);
    return () => clearInterval(interval);
  }, [client]);

  return (
    <div className="panel">
      <div className="panel-header">
        <h2>Agents</h2>
        <button onClick={refresh} className="btn-sm">
          Refresh
        </button>
      </div>
      {error && <div className="error">{error}</div>}
      {agents.length === 0 ? (
        <p className="muted">No agents running.</p>
      ) : (
        <table>
          <thead>
            <tr>
              <th>ID</th>
              <th>Provider</th>
              <th>Status</th>
              <th>CWD</th>
            </tr>
          </thead>
          <tbody>
            {agents.map((a) => (
              <tr
                key={a.id}
                onClick={() => onSelect(a.id)}
                className="clickable"
              >
                <td className="mono">{a.id.slice(0, 12)}...</td>
                <td>{a.provider}</td>
                <td>
                  <span className={`badge badge-${a.status}`}>{a.status}</span>
                </td>
                <td className="mono">{a.cwd}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
