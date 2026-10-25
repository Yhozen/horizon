import * as Alchemy from "alchemy";
import * as Cloudflare from "@alchemy.run/cloudflare";
import * as Effect from "effect/Effect";
import { readFile } from "node:fs/promises";
import { api } from "./scripts/cloudflare.ts";
import { source, allowedEmails } from "./scripts/config.mjs";
import { uploadAssets } from "./scripts/artifacts.mjs";

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
  const game = yield* Cloudflare.Worker("Game", {
    name: "horizon-web-preview",
    main: "./src/worker.ts",
    compatibility: { date: "2026-10-24" },
    workersDev: { enabled: true, previewsEnabled: false },
    access: {
      name: "Horizon private preview", allowedIdps: [login.identityProviderId], autoRedirectToIdentity: true,
      sessionDuration: "24h", policies: [{ decision: "allow", include: allowedEmails.map((email) => ({ email })) }],
    },
    cache: { enabled: true, crossVersionCache: true },
    entrypoints: { default: { cache: { enabled: false } }, Assets: { cache: { enabled: true } } },
    env: { BUCKET: bucket, ALLOWED_EMAILS: allowedEmails.join(","), RELEASE_ID: published.releaseId },
    observability: { enabled: true },
  });
  return { url: game.url, bucketName: bucket.bucketName, releaseId: published.releaseId };
}));
