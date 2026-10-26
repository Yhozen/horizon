import { DurableObject } from "cloudflare:workers";
import { closeSocket, openMinecraftPort } from "./multiplayer-port.ts";

type TcpConnection = Pick<Socket, "readable" | "writable" | "close" | "closed">;

// The connector chooses the one trusted destination; no client-controlled host is accepted.
export function bridgeWebSocket(open: () => Promise<TcpConnection>, waitUntil: (task: Promise<unknown>) => void): Response {
  const [client, server] = Object.values(new WebSocketPair());
  server.binaryType = "arraybuffer";
  server.accept();
  let closed = false, pending = 0, connection: TcpConnection | undefined;
  const finish = (code = 1000, reason = "Server connection closed") => {
    if (closed) return;
    closed = true;
    try { server.close(code, reason); } catch {}
    if (connection) void closeSocket(connection);
  };
  const ready = open().then((socket) => {
    connection = socket;
    if (closed) { void closeSocket(socket); throw new Error("Client disconnected"); }
    void socket.closed.catch(() => finish(1011, "Minecraft server disconnected"));
    return socket.writable.getWriter();
  });
  let writes = ready.then(() => {});
  server.addEventListener("message", (event) => {
    if (closed) return;
    if (typeof event.data === "string") { finish(1003, "Binary Minecraft data required"); return; }
    const data = event.data;
    pending += data.byteLength;
    if (pending > 4 * 1024 * 1024) { finish(1009, "Connection buffer exceeded"); return; }
    // Preserve packet order even while the container is starting or a write is backpressured.
    writes = writes.then(async () => {
      if (!closed) await (await ready).write(new Uint8Array(data));
      pending -= data.byteLength;
    }).catch((error) => {
      console.warn("Minecraft write failed", String(error));
      finish(1011, "Could not send to Minecraft server");
    });
  });
  server.addEventListener("close", () => finish());
  server.addEventListener("error", () => finish(1011, "Browser connection failed"));
  waitUntil((async () => {
    try {
      await ready;
      const reader = connection!.readable.getReader();
      try {
        while (!closed) {
          const { value, done } = await reader.read();
          if (done) break;
          server.send(value);
        }
      } finally { reader.releaseLock(); }
      finish();
    } catch (error) {
      console.warn("Minecraft connection failed", String(error));
      finish(1011, "Minecraft server is unavailable; please try again");
    }
  })());
  // Handle a failed startup even if the browser has not sent any bytes yet.
  void writes.catch(() => {});
  return new Response(null, { status: 101, webSocket: client });
}

export class Minecraft extends DurableObject<unknown> {
  constructor(ctx: DurableObjectState, env: unknown) {
    super(ctx, env);
    // Reapply the timeout when a Worker deployment restarts the Durable Object.
    if (ctx.container?.running) {
      void ctx.blockConcurrencyWhile(() => ctx.container!.setInactivityTimeout(5 * 60 * 1000));
    }
  }

  async fetch(request: Request): Promise<Response> {
    if (request.headers.get("Upgrade")?.toLowerCase() !== "websocket") {
      return new Response("WebSocket required", { status: 426, headers: { "Cache-Control": "no-store" } });
    }
    return bridgeWebSocket(async () => {
      const container = this.ctx.container;
      if (!container) throw new Error("Minecraft container is not configured");
      return openMinecraftPort(container);
    }, (task) => this.ctx.waitUntil(task));
  }
}
