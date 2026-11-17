import { DurableObject } from "cloudflare:workers";
import { connect } from "cloudflare:sockets";
import { closeSocket, openExternalMinecraftPort, openMinecraftPort } from "./multiplayer-port.ts";
import type { World } from "./worlds.ts";
export { availableWorlds, parseWorld, WORLDS, type World } from "./worlds.ts";

type TcpConnection = Pick<Socket, "readable" | "writable" | "close" | "closed">;

export interface MultiplayerEnv {
  /** The shared Horizon world (open.rccpr.tech's `SharedWorld` entrypoint). */
  SHARED_WORLD?: Fetcher;
  /** The Java server Java Edition players join too (`host:port`). */
  MINECRAFT_SERVER?: string;
  MINECRAFT?: DurableObjectNamespace<Minecraft>;
}

// Java players use the same Java server address directly.
export async function connectMultiplayer(request: Request, env: MultiplayerEnv,
  waitUntil: (task: Promise<unknown>) => void, world?: World): Promise<Response> {
  // Without a choice (older pages), the shared world when bound, as before.
  const chosen = world ?? (env.SHARED_WORLD ? "shared" : "survival");
  if (chosen === "shared") {
    if (!env.SHARED_WORLD) throw new Error("The shared world is not configured");
    return env.SHARED_WORLD.fetch(new Request("https://shared-world.internal/connect",
      { headers: { Upgrade: "websocket" } }));
  }
  const address = env.MINECRAFT_SERVER;
  if (address) {
    return bridgeWebSocket(() => openExternalMinecraftPort(address, connect), waitUntil);
  }
  if (!env.MINECRAFT) throw new Error("Minecraft server is not configured");
  return env.MINECRAFT.getByName("preview-world").fetch(request);
}

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

  // Clear any keepalive left by the retired Java relay.
  async alarm() { await this.ctx.storage.deleteAlarm(); }

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
