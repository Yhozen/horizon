// Local development only. multiplayer-dev.mjs binds this fixture to 127.0.0.1.
// The deployed Worker uses native Access identity and the container binding instead.
import { connect } from "cloudflare:sockets";
import { gateway } from "../src/http.ts";
import { bridgeWebSocket } from "../src/multiplayer.ts";
import { shell } from "../.build/shell.ts";

interface Env { TARGET: string; ASSET_ORIGIN: string }
export default {
  fetch(request: Request, env: Env, ctx: ExecutionContext) {
    return gateway(request, { ALLOWED_EMAILS: "local@localhost" }, {
      access: { getIdentity: async () => ({ email: "local@localhost" }) },
      exports: { Assets: { fetch: (incoming) => fetch(`${env.ASSET_ORIGIN}${new URL(incoming.url).pathname}`, {
        method: incoming.method, headers: incoming.headers,
      }) } },
    }, shell, async () => bridgeWebSocket(async () => {
      const socket = connect(env.TARGET);
      await socket.opened;
      return socket;
    }, (task) => ctx.waitUntil(task)));
  },
};
