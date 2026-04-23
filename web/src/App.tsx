import { useEffect, useState } from "react";
import { DaemonClient } from "./lib/ws";
import { AgentList } from "./components/AgentList";
import { TerminalView } from "./components/TerminalView";
import { SessionSearch } from "./components/SessionSearch";
import "./App.css";

type View = "agents" | "terminal" | "search";

function App() {
  const [client, setClient] = useState<DaemonClient | null>(null);
  const [connected, setConnected] = useState(false);
  const [view, setView] = useState<View>("agents");
  const [selectedAgent, setSelectedAgent] = useState<string | null>(null);

  useEffect(() => {
    const wsUrl = `ws://${window.location.host}/ws`;
    const c = new DaemonClient(wsUrl);
    c.connect()
      .then(() => {
        setClient(c);
        setConnected(true);
      })
      .catch(() => setConnected(false));

    return () => c.close();
  }, []);

  const handleSelectAgent = (id: string) => {
    setSelectedAgent(id);
    setView("terminal");
  };

  return (
    <div className="app">
      <header className="header">
        <h1>LeSearch</h1>
        <nav>
          <button
            className={view === "agents" ? "active" : ""}
            onClick={() => setView("agents")}
          >
            Agents
          </button>
          <button
            className={view === "search" ? "active" : ""}
            onClick={() => setView("search")}
          >
            Sessions
          </button>
        </nav>
        <span className={`status ${connected ? "connected" : "disconnected"}`}>
          {connected ? "Connected" : "Disconnected"}
        </span>
      </header>

      <main>
        {view === "agents" && (
          <AgentList client={client} onSelect={handleSelectAgent} />
        )}
        {view === "terminal" && (
          <TerminalView
            client={client}
            agentId={selectedAgent}
            onBack={() => setView("agents")}
          />
        )}
        {view === "search" && <SessionSearch client={client} />}
      </main>
    </div>
  );
}

export default App;
