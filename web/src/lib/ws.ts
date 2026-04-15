/**
 * JSON-RPC 2.0 WebSocket client for the LeSearch daemon.
 */

interface JsonRpcRequest {
  jsonrpc: "2.0";
  id: number;
  method: string;
  params: unknown;
}

type NotificationHandler = (method: string, params: unknown) => void;

export class DaemonClient {
  #ws: WebSocket | null = null;
  #nextId = 1;
  #pending = new Map<
    number,
    { resolve: (v: unknown) => void; reject: (e: Error) => void }
  >();
  #notificationHandler: NotificationHandler | null = null;
  #url: string;

  constructor(url: string) {
    this.#url = url;
  }

  /** Connect to the daemon WebSocket endpoint. */
  connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.#ws = new WebSocket(this.#url);

      this.#ws.onopen = () => resolve();
      this.#ws.onerror = () => reject(new Error("WebSocket connection failed"));

      this.#ws.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data as string);
          if ("id" in data && this.#pending.has(data.id)) {
            const pending = this.#pending.get(data.id)!;
            this.#pending.delete(data.id);
            if (data.error) {
              pending.reject(new Error(data.error.message));
            } else {
              pending.resolve(data.result);
            }
          } else if ("method" in data) {
            this.#notificationHandler?.(data.method, data.params);
          }
        } catch {
          // Ignore malformed messages
        }
      };

      this.#ws.onclose = () => {
        for (const { reject } of this.#pending.values()) {
          reject(new Error("connection closed"));
        }
        this.#pending.clear();
      };
    });
  }

  /** Send a JSON-RPC request and wait for the response. */
  call(method: string, params: unknown = {}): Promise<unknown> {
    return new Promise((resolve, reject) => {
      if (!this.#ws || this.#ws.readyState !== WebSocket.OPEN) {
        reject(new Error("not connected"));
        return;
      }

      const id = this.#nextId++;
      const request: JsonRpcRequest = {
        jsonrpc: "2.0",
        id,
        method,
        params,
      };

      this.#pending.set(id, { resolve, reject });
      this.#ws.send(JSON.stringify(request));
    });
  }

  /** Register a handler for notifications. */
  onNotification(handler: NotificationHandler): void {
    this.#notificationHandler = handler;
  }

  /** Check if connected. */
  get connected(): boolean {
    return this.#ws?.readyState === WebSocket.OPEN;
  }

  /** Close the connection. */
  close(): void {
    this.#ws?.close();
    this.#ws = null;
  }
}
