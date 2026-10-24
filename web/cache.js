const CACHE = "horizon-assets-v1";
const HISTORY = "horizon-releases-v1";

async function verifiedBytes(response, asset) {
  if (response.status !== 200 || response.redirected ||
      response.headers.get("content-type")?.split(";")[0] !== asset.contentType) {
    throw new Error(`Could not load ${asset.url}: unexpected response (${response.status})`);
  }
  const bytes = await response.arrayBuffer();
  if (bytes.byteLength !== asset.size) throw new Error(`Incomplete download: ${asset.url}`);
  const hash = Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)),
    (b) => b.toString(16).padStart(2, "0")).join("");
  if (hash !== asset.sha256) throw new Error(`Checksum mismatch: ${asset.url}`);
  return bytes;
}

export async function loadAsset(asset, {
  storage = globalThis.caches, fetcher = globalThis.fetch,
  origin = globalThis.location?.origin, report = () => {},
} = {}) {
  const key = new URL(asset.url, origin).href;
  let cache;
  try {
    cache = await storage?.open(CACHE);
    const hit = await cache?.match(key);
    if (hit) {
      try {
        const bytes = await verifiedBytes(hit, asset);
        report("cached", asset);
        return bytes;
      } catch {
        await cache.delete(key);
      }
    }
  } catch { report("unavailable", asset); }

  report("downloading", asset);
  // Cache Storage owns validated copies. Do not let an invalid HTTP-cache entry
  // survive a failed checksum and poison every retry of this immutable URL.
  const response = await fetcher(key, { credentials: "same-origin", redirect: "error", cache: "no-store" });
  const bytes = await verifiedBytes(response, asset);
  try {
    await cache?.put(key, new Response(bytes, { headers: {
      "Content-Type": asset.contentType,
      "Cache-Control": "private, max-age=31536000, immutable",
    } }));
  } catch { report("unavailable", asset); }
  return bytes;
}

// Only a successfully started release advances retention; a failed update keeps its predecessor.
export async function retainRelease(release, storage = globalThis.caches,
  origin = globalThis.location?.origin) {
  try {
    const history = await storage.open(HISTORY);
    const key = new URL("/__horizon_cache_history__", origin).href;
    const previous = await (await history.match(key))?.json() ?? [];
    const releases = [{ id: release.id, urls: [release.assets.wasm.url, release.assets.resources.url] },
      ...previous.filter((r) => r.id !== release.id)].slice(0, 2);
    await history.put(key, Response.json(releases));
    const keep = new Set(releases.flatMap((r) => r.urls.map((url) => new URL(url, origin).href)));
    const cache = await storage.open(CACHE);
    for (const request of await cache.keys()) {
      if (!keep.has(request.url)) await cache.delete(request);
    }
  } catch { /* Quota restrictions must not prevent the game from starting. */ }
}
