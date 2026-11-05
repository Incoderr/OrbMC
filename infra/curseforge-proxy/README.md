# CurseForge proxy

Cloudflare Worker between OrbMC and `api.curseforge.com`. The launcher never
carries the CurseForge API key; this Worker adds it. It also:

- allows only the endpoints the launcher uses (everything else is 403),
  only `gameId=432`, `pageSize` up to 50, POST batches up to 100 ids;
- caches GET answers at the edge (search 5 min, projects 10 min, categories
  1 day), so repeated requests do not reach CurseForge;
- limits each IP to 120 requests per minute.

Files are not proxied: download URLs point at the CurseForge CDN.

## Deploy

```bash
cd infra/curseforge-proxy
npx wrangler login
npx wrangler secret put CURSEFORGE_API_KEY
npx wrangler deploy
```

`deploy` prints the address (`https://orbmc-curseforge.<account>.workers.dev`).
Put it into the GitHub repository variable `ORBMC_CURSEFORGE_PROXY`
(Settings → Secrets and variables → Actions → Variables); release builds
bake it in.

Rotating the key: `wrangler secret put CURSEFORGE_API_KEY` again. No launcher
release is needed.

## Local launcher runs

- `ORBMC_CURSEFORGE_PROXY=<address>` — through the proxy;
- `CURSEFORGE_API_KEY=<own key>` — straight to the API, no proxy.

## Recommended Cloudflare settings

Security → Bots: Bot Fight Mode on. For abuse from hosting networks add a WAF
rule that blocks or challenges requests from data-center ASNs to this Worker.
