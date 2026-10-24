import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
import { parseArgs } from "node:util";
import { packageRelease } from "./artifacts.mjs";

const { values } = parseArgs({ options: { archive: { type: "string" } } });
const release = await packageRelease(fileURLToPath(new URL("..", import.meta.url)),
  values.archive ? resolve(values.archive) : undefined);
console.log(`Packaged release ${release.id}`);
