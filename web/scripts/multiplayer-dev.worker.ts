// Local development only. multiplayer-dev.mjs binds this fixture to 127.0.0.1.
// The deployed Worker uses native Access identity and the configured backends: here
// TARGET stands in for the Java server (survival) and SHARED, when set, for the shared
// Horizon world, both bridged to local TCP ports.
import { connect } from "cloudflare:sockets";
import { gateway } from "../src/http.ts";
import { availableWorlds, bridgeWebSocket, connectMultiplayer, parseWorld, type MultiplayerEnv } from "../src/multiplayer.ts";
import { openExternalMinecraftPort } from "../src/multiplayer-port.ts";
import { shell } from "../.build/shell.ts";

interface Env { TARGET: string; SHARED?: string; ASSET_ORIGIN: string }
export default {
  fetch(request: Request, env: Env, ctx: ExecutionContext) {
    const waitUntil = (task: Promise<unknown>) => ctx.waitUntil(task);
    const shared = env.SHARED;
    const backends: MultiplayerEnv = { MINECRAFT_SERVER: env.TARGET, ...(shared && { SHARED_WORLD: {
      fetch: async () => bridgeWebSocket(() => openExternalMinecraftPort(shared, connect), waitUntil),
    } as unknown as Fetcher }) };
    return gateway(request, { ALLOWED_EMAILS: "local@localhost" }, {
      access: { getIdentity: async () => ({ email: "local@localhost" }) },
      exports: { Assets: { fetch: (incoming) => fetch(`${env.ASSET_ORIGIN}${new URL(incoming.url).pathname}`, {
        method: incoming.method, headers: incoming.headers,
      }) } },
    }, shell, (connection, world) => connectMultiplayer(connection, backends, waitUntil, parseWorld(world)),
    availableWorlds(backends));
  },
};
