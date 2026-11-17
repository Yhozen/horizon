// Horizon Open Creative's public site (docs/open-creative.md): the page and its
// scripts from the bundled shell, the game's immutable assets from R2, and
// /connect, the browser's WebSocket to the shared open server (Horizon's own
// dedicated server on the open bundle, in a container). Everything it serves is
// openly licensed or Horizon's own, so there is no sign-in.
import { DurableObject, WorkerEntrypoint } from "cloudflare:workers";
import { failure } from "./http.ts";
import { route } from "./open-route.ts";
import { bridgeWebSocket } from "./multiplayer.ts";
import { openMinecraftPort } from "./multiplayer-port.ts";
import { shell } from "../.build-open/shell.ts";

interface Env {
  BUCKET: R2Bucket;
  OPEN_SERVER?: DurableObjectNamespace<OpenServer>;
  /** The translating relay in front of the Java server (crates/horizon-open-relay, in a
   * container): open players join that world through it. Unbound, the menu doesn't offer it. */
  SURVIVAL_RELAY?: DurableObjectNamespace<SurvivalRelay>;
}

/** The one shared open world. */
export const WORLD = "open-world";

/** Forward only the upgrade: the browser's headers never reach a server. */
const upgrade = (host: string) => new Request(`https://${host}/connect`, { headers: { Upgrade: "websocket" } });

export default {
  fetch(request: Request, env: Env) {
    const server = env.OPEN_SERVER, relay = env.SURVIVAL_RELAY;
    return route(request, env.BUCKET, shell, {
      ...(server && { shared: () => server.getByName(WORLD).fetch(upgrade("open-server.internal")) }),
      ...(relay && { survival: () => relay.getByName("survival").fetch(upgrade("survival-relay.internal")) }),
    });
  },
};

/** The shared world for the private preview's Worker (a service binding to this
 * entrypoint, never public): the same server and world as this site's /connect. */
export class SharedWorld extends WorkerEntrypoint<Env> {
  fetch(request: Request) {
    if (request.headers.get("Upgrade")?.toLowerCase() !== "websocket") return failure("WebSocket required", 426);
    if (!this.env.OPEN_SERVER) return failure("Multiplayer unavailable", 503);
    return this.env.OPEN_SERVER.getByName(WORLD).fetch(upgrade("open-server.internal"));
  }
}

/** A Minecraft server in this Durable Object's container, bridged to the browser's WebSocket. */
abstract class ContainerServer extends DurableObject<Env> {
  /** Whether the container may reach the internet. */
  protected abstract readonly internet: boolean;

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    if (ctx.container?.running) {
      void ctx.blockConcurrencyWhile(() => ctx.container!.setInactivityTimeout(10 * 60 * 1000));
    }
  }

  async fetch(request: Request): Promise<Response> {
    if (request.headers.get("Upgrade")?.toLowerCase() !== "websocket") return failure("WebSocket required", 426);
    return bridgeWebSocket(async () => {
      const container = this.ctx.container;
      if (!container) throw new Error("The server is not configured");
      const socket = await openMinecraftPort(container, 360, 500, this.internet);
      await container.setInactivityTimeout(10 * 60 * 1000);
      return socket;
    }, (task) => this.ctx.waitUntil(task));
  }
}

/** The container running `horizon-client --dedicated` on the open bundle (web/open-server).
 * It needs no internet access of its own. */
export class OpenServer extends ContainerServer {
  protected readonly internet = false;
}

/** The container running the relay to the Java server (crates/horizon-open-relay/deploy).
 * It needs the internet: for the Java server, and on first start for Mojang's server jar,
 * from which it derives the vanilla numbering on its own disk. */
export class SurvivalRelay extends ContainerServer {
  protected readonly internet = true;
}
