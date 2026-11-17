import { WorkerEntrypoint } from "cloudflare:workers";
import { gateway, serveAsset, type GatewayContext, type GatewayEnv } from "./http.ts";
import { shell } from "../.build/shell.ts";
import { availableWorlds, connectMultiplayer, parseWorld, type MultiplayerEnv } from "./multiplayer.ts";
export { Minecraft } from "./multiplayer.ts";

interface Env extends GatewayEnv, MultiplayerEnv { BUCKET: R2Bucket }

// Only the default export has a public URL. This entrypoint is invoked after authorization.
export class Assets extends WorkerEntrypoint<Env> {
  fetch(request: Request) { return serveAsset(request, this.env.BUCKET); }
}

export default {
  fetch(request: Request, env: Env, ctx: GatewayContext & Pick<ExecutionContext, "waitUntil">) {
    return gateway(request, env, ctx, shell,
      (connection, world) => connectMultiplayer(connection, env, (task) => ctx.waitUntil(task), parseWorld(world)),
      availableWorlds(env));
  },
};
