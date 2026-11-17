// The worlds a browser may choose by name on /connect?world=. A name selects one of the
// deployment's configured backends; request URLs never carry a host or port. No
// Cloudflare runtime imports, so the tests can load it.
export const WORLDS = ["survival", "shared"] as const;
export type World = typeof WORLDS[number];

export function parseWorld(value: string | null): World | undefined {
  return WORLDS.find((world) => world === value);
}

/** The worlds this deployment serves, for the page's menu (/worlds.json). */
export function availableWorlds(env: { SHARED_WORLD?: unknown; MINECRAFT_SERVER?: string; MINECRAFT?: unknown }): World[] {
  return WORLDS.filter((world) => world === "shared" ? Boolean(env.SHARED_WORLD)
    : Boolean(env.MINECRAFT_SERVER || env.MINECRAFT));
}
