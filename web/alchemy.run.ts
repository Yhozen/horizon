import * as Alchemy from "alchemy";
import * as Cloudflare from "@alchemy.run/cloudflare";
import * as Effect from "effect/Effect";
import { readFile } from "node:fs/promises";
import { api } from "./scripts/cloudflare.ts";
import { source, allowedEmails, javaServer } from "./scripts/config.mjs";
import { uploadAssets } from "./scripts/artifacts.mjs";
import { minecraftServer } from "./scripts/multiplayer-config.mjs";

interface Upload {
  releaseId: string;
  files: { key: string; path: string; sha256: string; md5: string; size: number; contentType: string }[];
}

const PublishAssets = Alchemy.Action("PublishAssets", Effect.gen(function* () {
  const credentials = yield* Cloudflare.CloudflareEnvironment.current;
  return (input: Upload & { bucket: string }) => Effect.tryPromise(async () => {
    await uploadAssets(api(credentials), input.bucket, input.files);
    return { releaseId: input.releaseId };
  });
}));

export default Alchemy.Stack("HorizonWeb", {
  providers: Cloudflare.providers(), state: Cloudflare.state(),
}, Effect.gen(function* () {
  const { stage } = yield* Alchemy.Stack;
  if (stage !== "preview") return yield* Effect.die(new Error("Deploy this stack with --stage preview"));
  const upload = yield* Effect.tryPromise(async () => JSON.parse(
    await readFile(new URL("./.build/upload.json", import.meta.url), "utf8")) as Upload).pipe(Effect.orDie);
  const bucket = yield* Cloudflare.R2.Bucket.ref(source.resource, { stack: source.stack, stage: source.stage });
  const login = yield* Cloudflare.Access.IdentityProvider("EmailCode", {
    name: "Horizon email code", type: "onetimepin", config: {},
  });
  const published = yield* PublishAssets({ ...upload, bucket: bucket.bucketName });
  const server = yield* Effect.tryPromise(() => minecraftServer(undefined, undefined,
    `${javaServer.hostname}:${javaServer.port}`)).pipe(Effect.orDie);
  const game = yield* Cloudflare.Worker("Game", {
    name: "horizon-web-preview",
    domain: "horizon.rccpr.tech",
    main: "./src/worker.ts",
    compatibility: { date: "2026-10-24" },
    workersDev: { enabled: true, previewsEnabled: false },
    access: {
      name: "Horizon private preview", allowedIdps: [login.identityProviderId], autoRedirectToIdentity: true,
      sessionDuration: "24h", policies: [{ decision: "allow", include: allowedEmails.map((email) => ({ email })) }],
    },
    cache: { enabled: true, crossVersionCache: true },
    entrypoints: { default: { cache: { enabled: false } }, Assets: { cache: { enabled: true } } },
    env: {
      BUCKET: bucket, ALLOWED_EMAILS: allowedEmails.join(","), RELEASE_ID: published.releaseId,
      // Multiplayer is the shared Horizon world that open.rccpr.tech's players use too: its
      // Worker (web/scripts/deploy-open.mjs, outside this stack) serves it to this binding.
      SHARED_WORLD: Cloudflare.WorkerEntrypoint(Cloudflare.Worker.lookup({ name: "horizon-open" }), "SharedWorld"),
      MINECRAFT_SERVER: server,
      MINECRAFT: Cloudflare.Container("Minecraft", {
        className: "Minecraft",
        image: "docker.io/itzg/minecraft-server@sha256:0501cb2609925a658432eb31b4a64452c3f37761893f5873ec60df80c91e434e",
        instanceType: "standard-2", instances: 0, maxInstances: 1,
        observability: { logs: { enabled: true } },
        env: {
          EULA: "TRUE", TYPE: "VANILLA", VERSION: "26.3", ONLINE_MODE: "FALSE",
          ENFORCE_SECURE_PROFILE: "FALSE", MEMORY: "1536M", VIEW_DISTANCE: "4",
          SIMULATION_DISTANCE: "4", MODE: "survival", FORCE_GAMEMODE: "TRUE", DIFFICULTY: "normal",
          MAX_PLAYERS: "10", SPAWN_PROTECTION: "0", ENABLE_RCON: "FALSE", ENABLE_WHITELIST: "FALSE",
        },
      }),
    },
    observability: { enabled: true },
  });
  if (javaServer.address) {
    yield* Cloudflare.DNS.Record("JavaHostname", {
      zoneId: javaServer.zoneId, type: "A", name: javaServer.hostname,
      content: javaServer.address, proxied: false, ttl: 60,
    });
    yield* Cloudflare.DNS.Record("JavaDiscovery", {
      zoneId: javaServer.zoneId, type: "SRV", name: `_minecraft._tcp.${javaServer.hostname}`,
      content: { priority: 0, weight: 0, port: javaServer.port, target: javaServer.hostname }, ttl: 60,
    });
  }
  return { url: game.url, bucketName: bucket.bucketName, releaseId: published.releaseId,
    javaServer: server === `${javaServer.hostname}:${javaServer.port}` ? javaServer.hostname
      : server || "Not configured; browser-only Cloudflare container" };
}));
