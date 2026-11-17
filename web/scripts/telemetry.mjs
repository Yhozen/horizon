import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { parseEnv } from "node:util";
import { execFileSync } from "node:child_process";
import { build } from "rolldown";

export async function bundleTelemetry(root, { app = "horizon-web", stage = "preview" } = {}) {
  const input = join(root, "telemetry.js");
  try { await readFile(input); } catch (error) { if (error.code === "ENOENT") return; throw error; }
  let local = {};
  try { local = parseEnv(await readFile(join(root, ".env.local"), "utf8")); }
  catch (error) { if (error.code !== "ENOENT") throw error; }
  // Explicit local configuration wins over inherited credentials from another project.
  const env = { ...process.env, ...local };
  const token = env.NEXT_PUBLIC_POSTHOG_PROJECT_TOKEN ?? "";
  const host = env.NEXT_PUBLIC_POSTHOG_HOST ?? "";
  if (Boolean(token) !== Boolean(host)) throw new Error("Set both NEXT_PUBLIC_POSTHOG_PROJECT_TOKEN and NEXT_PUBLIC_POSTHOG_HOST");
  if (token && !token.startsWith("phc_")) throw new Error("Use a public PostHog project token (phc_), not a personal API key");
  if (host && new URL(host).protocol !== "https:") throw new Error("PostHog ingestion requires HTTPS");
  let commit = "unknown";
  try { commit = execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim(); } catch {}
  const result = await build({ input, platform: "browser", write: false,
    output: { format: "esm", minify: true, codeSplitting: false,
      banner: `const __HORIZON_POSTHOG__ = ${JSON.stringify({ token, host, commit, stage, app })};` } });
  return result.output.find((file) => file.type === "chunk").code;
}
