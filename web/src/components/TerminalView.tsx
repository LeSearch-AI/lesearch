import { useEffect, useRef, useState } from "react";
import type { DaemonClient } from "../lib/ws";

interface Props {
  client: DaemonClient | null;
  agentId: string | null;
  onBack: () => void;
}

interface TerminalLine {
  type: "output" | "status" | "tool" | "done";
  text: string;
  timestamp: string;
}

export function TerminalView({ client, agentId, onBack }: Props) {
  const [lines, setLines] = useState<TerminalLine[]>([]);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!client || !agentId) return;

    const handler = (method: string, params: unknown) => {
      const p = params as Record<string, unknown>;
      const now = new Date().toLocaleTimeString();

      switch (method) {
        case "agent.output":
          setLines((prev) => [
            ...prev,
            {
              type: "output",
              text: (p.data as string) ?? "",
              timestamp: now,
            },
          ]);
          break;
        case "agent.status":
          setLines((prev) => [
            ...prev,
            {
              type: "status",
              text: `[status: ${p.state}]`,
              timestamp: now,
            },
          ]);
          break;
        case "agent.tool_call": {
          const tc = p.tool_call as Record<string, unknown> | undefined;
          setLines((prev) => [
            ...prev,
            {
              type: "tool",
              text: `[tool: ${tc?.tool_name ?? "?"}]`,
              timestamp: now,
            },
          ]);
          break;
        }
        case "agent.done":
          setLines((prev) => [
            ...prev,
            {
              type: "done",
              text: `[done, exit code: ${p.exit_code ?? -1}]`,
              timestamp: now,
            },
          ]);
          break;
      }
    };

    client.onNotification(handler);
    return () => client.onNotification(() => {});
  }, [client, agentId]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [lines]);

  return (
    <div className="panel">
      <div className="panel-header">
        <h2>
          Agent{" "}
          <span className="mono">{agentId?.slice(0, 12) ?? "?"}...</span>
        </h2>
        <button onClick={onBack} className="btn-sm">
          Back
        </button>
      </div>
      <div className="terminal">
        {lines.map((line, i) => (
          <div key={i} className={`terminal-line terminal-${line.type}`}>
            {line.text}
          </div>
        ))}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
