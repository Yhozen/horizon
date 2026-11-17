import { readFile } from "node:fs/promises";
import { isIP } from "node:net";
import { parseEnv } from "node:util";

export function parseMinecraftServer(value = "") {
  if (!value.trim()) return "";
  const match = /^(\[[0-9a-fA-F:.]+\]|[a-zA-Z0-9.-]+):([0-9]{1,5})$/.exec(value.trim());
  const invalid = () => new Error("HORIZON_MINECRAFT_SERVER must be HOST:PORT, for example mc.example.com:25565");
  if (!match) throw invalid();
  const [, host, port] = match;
  if (Number(port) < 1 || Number(port) > 65535) throw invalid();
  if (host.startsWith("[")) {
    if (isIP(host.slice(1, -1)) !== 6) throw invalid();
  } else if (host.length > 253 || !host.split(".").every(label =>
    /^[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?$/.test(label))) throw invalid();
  return `${host.toLowerCase()}:${Number(port)}`;
}

export async function minecraftServer(env = process.env, file = new URL("../.env.local", import.meta.url), fallback = "") {
  let local = {};
  try { local = parseEnv(await readFile(file, "utf8")); }
  catch (error) { if (error.code !== "ENOENT") throw error; }
  return parseMinecraftServer(local.HORIZON_MINECRAFT_SERVER ?? env.HORIZON_MINECRAFT_SERVER ?? fallback);
}

