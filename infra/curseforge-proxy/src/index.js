// CurseForge API proxy for OrbMC. The launcher never carries the API key:
// it calls this Worker, which checks the request against the endpoints the
// launcher actually uses, adds the key and caches the answer at the edge.

const UPSTREAM = "https://api.curseforge.com";
const MINECRAFT = "432";

// Only what `app/src/features/mods` sends. Everything else is 403.
const ROUTES = [
  { method: "GET", path: /^\/v1\/mods\/search$/, ttl: 300 },
  { method: "GET", path: /^\/v1\/categories$/, ttl: 86400 },
  { method: "GET", path: /^\/v1\/mods\/\d+$/, ttl: 600 },
  { method: "GET", path: /^\/v1\/mods\/\d+\/description$/, ttl: 3600 },
  { method: "GET", path: /^\/v1\/mods\/\d+\/files$/, ttl: 300 },
  { method: "GET", path: /^\/v1\/mods\/\d+\/files\/\d+\/download-url$/, ttl: 3600 },
  { method: "POST", path: /^\/v1\/mods$/, ttl: 0, body: "modIds" },
  { method: "POST", path: new RegExp(`^/v1/fingerprints/${MINECRAFT}$`), ttl: 0, body: "fingerprints" },
];

const MAX_PAGE_SIZE = 50;
const MAX_BATCH = 100;
const MAX_BODY_BYTES = 16 * 1024;

export default {
  async fetch(request, env, ctx) {
    const url = new URL(request.url);
    const route = ROUTES.find(
      (r) => r.method === request.method && r.path.test(url.pathname),
    );
    if (!route) return error(403, "endpoint not allowed");

    // Per-IP limit (Workers Rate Limiting binding, see wrangler.toml).
    const ip = request.headers.get("cf-connecting-ip") ?? "unknown";
    if (env.LIMITER) {
      const { success } = await env.LIMITER.limit({ key: ip });
      if (!success) return error(429, "too many requests");
    }

    if (request.method === "GET") {
      const problem = checkQuery(url);
      if (problem) return error(400, problem);
      return cachedGet(url, route.ttl, env, ctx);
    }

    const body = await readBody(request, route.body);
    if (typeof body === "string") return error(400, body);
    return forward(url, env, { method: "POST", body: JSON.stringify(body) });
  },
};

function checkQuery(url) {
  const gameId = url.searchParams.get("gameId");
  if (gameId !== null && gameId !== MINECRAFT) return "only Minecraft is served";
  const pageSize = Number(url.searchParams.get("pageSize") ?? 0);
  if (pageSize > MAX_PAGE_SIZE) return `pageSize is limited to ${MAX_PAGE_SIZE}`;
  return null;
}

// POST bodies are small id lists; anything else is refused before it costs
// an upstream request.
async function readBody(request, key) {
  const text = await request.text();
  if (text.length > MAX_BODY_BYTES) return "body too large";
  let json;
  try {
    json = JSON.parse(text);
  } catch {
    return "body is not JSON";
  }
  const list = json?.[key];
  if (!Array.isArray(list) || list.length === 0 || list.length > MAX_BATCH)
    return `expected 1..${MAX_BATCH} ids`;
  if (!list.every((id) => Number.isSafeInteger(id) && id >= 0)) return "ids must be numbers";
  return { [key]: list };
}

async function cachedGet(url, ttl, env, ctx) {
  // The cache key is the normalized public URL; the key header never
  // reaches the cache.
  url.searchParams.sort();
  const cacheKey = new Request(url.toString(), { method: "GET" });
  const cache = caches.default;
  const hit = await cache.match(cacheKey);
  if (hit) return hit;

  const response = await forward(url, env, { method: "GET" });
  if (response.ok && ttl > 0) {
    const stored = new Response(response.body, response);
    stored.headers.set("Cache-Control", `public, max-age=${ttl}`);
    ctx.waitUntil(cache.put(cacheKey, stored.clone()));
    return stored;
  }
  return response;
}

async function forward(url, env, init) {
  if (!env.CURSEFORGE_API_KEY) return error(503, "proxy is not configured");
  const upstream = await fetch(`${UPSTREAM}${url.pathname}${url.search}`, {
    ...init,
    headers: {
      "x-api-key": env.CURSEFORGE_API_KEY,
      accept: "application/json",
      ...(init.body ? { "content-type": "application/json" } : {}),
    },
  });
  // Pass status and body through, drop upstream headers.
  return new Response(upstream.body, {
    status: upstream.status,
    headers: { "content-type": "application/json" },
  });
}

function error(status, message) {
  return new Response(JSON.stringify({ error: message }), {
    status,
    headers: { "content-type": "application/json" },
  });
}
