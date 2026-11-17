// Local development only (pnpm serve:open:multiplayer binds it to 127.0.0.1): the open
// site from serve-open.mjs, and /connect bridged to a local open server's TCP port,
// as the deployed Worker bridges it to its container.
import { connect } from "cloudflare:sockets";
import { failure } from "../src/http.ts";
import { bridgeWebSocket } from "../src/multiplayer.ts";
import { openExternalMinecraftPort } from "../src/multiplayer-port.ts";
import { route } from "../src/open-route.ts";

interface Env { TARGET: string; ASSET_ORIGIN: string; RELAY?: string }
const empty = {} as R2Bucket;
export default {
  fetch(request: Request, env: Env, ctx: ExecutionContext) {
    const url = new URL(request.url);
    if (url.pathname === "/connect" || url.pathname === "/worlds.json") {
      const bridge = (target: string) => async () =>
        bridgeWebSocket(() => openExternalMinecraftPort(target, connect), (task) => ctx.waitUntil(task));
      return route(request, empty, {}, { shared: bridge(env.TARGET), ...(env.RELAY && { survival: bridge(env.RELAY) }) });
    }
    if (request.method !== "GET" && request.method !== "HEAD") return failure("Method not allowed", 405);
    return fetch(`${env.ASSET_ORIGIN}${url.pathname}`, { method: request.method, headers: request.headers });
  },
};
