import { useState } from "react";
import type { DaemonClient } from "../lib/ws";

interface SearchHit {
  session_id: string;
  agent_id: string;
  event_type: string;
  timestamp: string;
  data: string;
}

interface Props {
  client: DaemonClient | null;
}

export function SessionSearch({ client }: Props) {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [searched, setSearched] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const search = async () => {
    if (!client?.connected || !query.trim()) return;
    try {
      const result = (await client.call("session.search", {
        query,
        limit: 50,
      })) as { hits: SearchHit[] };
      setHits(result.hits ?? []);
      setSearched(true);
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") search();
  };

  return (
    <div className="panel">
      <div className="panel-header">
        <h2>Session Search</h2>
      </div>
      <div className="search-bar">
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Search sessions..."
        />
        <button onClick={search} className="btn-sm">
          Search
        </button>
      </div>
      {error && <div className="error">{error}</div>}
      {searched && hits.length === 0 && (
        <p className="muted">No results found.</p>
      )}
      {hits.length > 0 && (
        <table>
          <thead>
            <tr>
              <th>Timestamp</th>
              <th>Type</th>
              <th>Session</th>
            </tr>
          </thead>
          <tbody>
            {hits.map((h, i) => (
              <tr key={i}>
                <td className="mono">{h.timestamp}</td>
                <td>{h.event_type}</td>
                <td className="mono">{h.session_id.slice(0, 12)}...</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
