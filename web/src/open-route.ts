// Horizon Open Creative's routes (src/open-worker.ts): no Cloudflare runtime
// imports, so tests/open.test.mjs can exercise them.
import { assetPath, failure, serveAsset, worldList, type Shell } from "./http.ts";

/** A configured world's connector, by the name the page's menu uses. */
export type Worlds = Readonly<Partial<Record<"shared" | "survival", () => Promise<Response>>>>;

export function route(request: Request, bucket: R2Bucket, pages: Shell,
  worlds: Worlds = {}): Promise<Response> | Response {
  const url = new URL(request.url);
  if (url.pathname === "/connect") {
    if (request.method !== "GET" || request.headers.get("Upgrade")?.toLowerCase() !== "websocket") {
      return failure("WebSocket required", 426);
    }
    // Only this site's page may open game connections.
    if (request.headers.get("Origin") !== url.origin) return failure("Invalid connection origin", 403);
    // Older pages name no world: the shared one.
    const name = url.searchParams.get("world") ?? "shared";
    const connect = name === "shared" || name === "survival" ? worlds[name] : undefined;
    if (!connect) return failure("Unknown world", 404);
    return connect();
  }
  if (url.pathname === "/worlds.json") return worldList(Object.keys(worlds));
  if (request.method !== "GET" && request.method !== "HEAD") return failure("Method not allowed", 405);
  if (assetPath.test(url.pathname)) return serveAsset(request, bucket);
  const page = pages[url.pathname === "/" ? "/index.html" : url.pathname];
  if (!page) return failure("Not found", 404);
  return new Response(request.method === "HEAD" ? null : page.body, { headers: {
    "Content-Type": `${page.contentType}; charset=utf-8`,
    "Cache-Control": "no-cache", "X-Content-Type-Options": "nosniff",
  } });
}
