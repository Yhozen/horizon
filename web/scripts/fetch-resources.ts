import { fileURLToPath } from "node:url";
import { api, profileCredentials } from "./cloudflare.ts";
import { fetchResources } from "./artifacts.mjs";
import { source } from "./config.mjs";

const digest = await fetchResources(api(await profileCredentials()), source,
  fileURLToPath(new URL("../.build/horizon-data.bin", import.meta.url)));
console.log(`Resource archive ready: ${digest.size} bytes, SHA-256 ${digest.sha256}`);
