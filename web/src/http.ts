export const YEAR = 31536000;
export const assetPath = /^\/assets\/[a-f0-9]{64}\/(horizon-data\.bin|horizon-client_bg\.wasm|horizon-client\.js)$/;
export type Shell = Readonly<Record<string, { readonly body: string; readonly contentType: string }>>;

export function failure(message: string, status: number) {
  return new Response(message, { status, headers: { "Cache-Control": "no-store" } });
}

export async function serveAsset(request: Request, bucket: R2Bucket): Promise<Response> {
  const url = new URL(request.url);
  if (!assetPath.test(url.pathname) || url.search) return failure("Not found", 404);
  if (request.method !== "GET" && request.method !== "HEAD") return failure("Method not allowed", 405);
  try {
    const key = url.pathname.slice(1);
    const object = request.method === "HEAD" ? await bucket.head(key) : await bucket.get(key);
    if (!object) return failure("Not found", 404);
    const headers = new Headers();
    object.writeHttpMetadata(headers);
    headers.set("ETag", object.httpEtag);
    headers.set("Content-Length", String(object.size));
    headers.set("Cache-Control", `public, max-age=${YEAR}, immutable`);
    headers.set("X-Content-Type-Options", "nosniff");
    const body = "body" in object ? (object as R2ObjectBody).body : null;
    if (request.headers.get("If-None-Match")?.split(",").map((v) => v.trim()).includes(object.httpEtag)) {
      await body?.cancel();
      headers.delete("Content-Length");
      return new Response(null, { status: 304, headers });
    }
    return new Response(body, { headers });
  } catch { return failure("Asset storage unavailable", 502); }
}

export interface GatewayEnv {
  ALLOWED_EMAILS: string;
}
export interface GatewayContext {
  access?: { getIdentity(): Promise<{ email?: string } | undefined> };
  exports: { Assets: { fetch(request: Request): Promise<Response> } };
}

export async function gateway(request: Request, env: GatewayEnv, ctx: GatewayContext, shell: Shell,
  multiplayer?: (request: Request) => Promise<Response>): Promise<Response> {
  // The platform-populated Access context cannot be forged by supplying HTTP headers.
  try {
    const identity = await ctx.access?.getIdentity();
    const allowed = env.ALLOWED_EMAILS.split(",").map((email) => email.trim().toLowerCase());
    if (!identity?.email || !allowed.includes(identity.email.toLowerCase())) {
      return failure("Access required", 403);
    }
  } catch { return failure("Could not verify access", 503); }
  if (request.method !== "GET" && request.method !== "HEAD") return failure("Method not allowed", 405);
  try {
    const url = new URL(request.url);
    if (url.pathname === "/connect") {
      if (request.method !== "GET" || request.headers.get("Upgrade")?.toLowerCase() !== "websocket") {
        return failure("WebSocket required", 426);
      }
      if (request.headers.get("Origin") !== url.origin) return failure("Invalid connection origin", 403);
      if (!multiplayer) return failure("Multiplayer unavailable", 503);
      // Forward only the upgrade. Access cookies and assertions never reach the server.
      return await multiplayer(new Request("https://minecraft.internal/connect", { headers: { Upgrade: "websocket" } }));
    }
    return await route(request, ctx, shell);
  } catch { return failure("Content unavailable", 502); }
}

async function route(request: Request, ctx: GatewayContext, shell: Shell): Promise<Response> {
  const url = new URL(request.url);
  if (assetPath.test(url.pathname) && !url.search) {
    // Authentication has completed. Only content identity participates in the internal cache key.
    const headers = new Headers();
    for (const name of ["Range", "If-None-Match"]) {
      const value = request.headers.get(name);
      if (value) headers.set(name, value);
    }
    const response = await ctx.exports.Assets.fetch(new Request(`https://assets.internal${url.pathname}`,
      { method: request.method, headers }));
    const outgoing = new Headers(response.headers);
    outgoing.set("X-Horizon-Asset-Cache", response.headers.get("Cf-Cache-Status") ?? "UNKNOWN");
    outgoing.set("Cache-Control", response.ok || response.status === 304
      ? `private, max-age=${YEAR}, immutable` : "no-store");
    return new Response(response.body, { status: response.status, headers: outgoing });
  }
  const page = shell[url.pathname === "/" ? "/index.html" : url.pathname];
  if (!page) return failure("Not found", 404);
  return new Response(request.method === "HEAD" ? null : page.body, { headers: {
    "Content-Type": `${page.contentType}; charset=utf-8`,
    "Content-Length": String(new TextEncoder().encode(page.body).byteLength),
    "Cache-Control": "no-store", "X-Content-Type-Options": "nosniff",
  } });
}
