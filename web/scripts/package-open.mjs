// pnpm package:open: prepare the open bundle again and package it with the
// existing WASM build (web/.build-open/dist), without rebuilding the game.
import { fileURLToPath } from "node:url";
import { packageOpenRelease, prepareOpen } from "./open.mjs";

const web = fileURLToPath(new URL("..", import.meta.url));
prepareOpen(web);
const release = await packageOpenRelease(web);
console.log(`Horizon Open Creative: packaged release ${release.id} in web/.build-open/static`);
