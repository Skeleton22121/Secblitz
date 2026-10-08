// The count is every download of a Secblitz installer or portable app from the
// GitHub releases. The last total is kept in D1, so the number still shows while
// GitHub is slow or limits how often it may be asked.
const RELEASES = "https://api.github.com/repos/secblitz/Secblitz/releases?per_page=100";
const FILE = /^secblitz-\d+\.\d+\.\d+-windows-(?:x64|arm64)(?:-setup)?\.exe$/;
const REFRESH = 10 * 60 * 1000;
const PAGES = 10;
const HEADERS = {
  "Cache-Control": "public, max-age=300",
  "Content-Security-Policy": "default-src 'none'; frame-ancestors 'none'",
  "X-Content-Type-Options": "nosniff",
  "Referrer-Policy": "no-referrer",
  "Strict-Transport-Security": "max-age=31536000",
};

async function releaseDownloads() {
  let total = 0;
  for (let page = 1; page <= PAGES; page++) {
    const response = await fetch(`${RELEASES}&page=${page}`, {
      headers: {
        Accept: "application/vnd.github+json",
        "User-Agent": "secblitz-download-counter",
        "X-GitHub-Api-Version": "2022-11-28",
      },
    });
    if (!response.ok) throw new Error(`GitHub releases answered ${response.status}`);
    const releases = await response.json();
    for (const release of releases) {
      for (const asset of release.assets) {
        if (FILE.test(asset.name)) total += asset.download_count;
      }
    }
    if (releases.length < 100) return total;
  }
  throw new Error("GitHub releases span more pages than expected");
}

// Claiming the row first means visitors arriving together ask GitHub once.
async function refresh(env) {
  const now = Date.now();
  const claim = await env.DB.prepare("UPDATE release_downloads SET checked = ?1 WHERE id = 1 AND checked <= ?2")
    .bind(now, now - REFRESH)
    .run();
  if (!claim.meta.changes) return;
  const total = await releaseDownloads();
  await env.DB.prepare("UPDATE release_downloads SET total = ?1 WHERE id = 1").bind(total).run();
}

function short(n) {
  if (n < 1000) return String(n);
  if (n < 1000000) return `${(n / 1000).toFixed(n < 10000 ? 1 : 0).replace(/\.0$/, "")}k`;
  return `${(n / 1000000).toFixed(1).replace(/\.0$/, "")}M`;
}

// ?format=badge answers in the shields.io endpoint format for the README.
export async function onRequestGet({ request, env, waitUntil }) {
  const row = await env.DB.prepare("SELECT total, checked FROM release_downloads WHERE id = 1").first();
  if (row && row.checked === 0) {
    await refresh(env).catch((error) => console.error(error));
  } else {
    waitUntil(refresh(env).catch((error) => console.error(error)));
  }
  const current = await env.DB.prepare("SELECT total FROM release_downloads WHERE id = 1").first();
  const total = Number(current?.total ?? 0);
  const body =
    new URL(request.url).searchParams.get("format") === "badge"
      ? { schemaVersion: 1, label: "downloads", message: short(total), color: "18181B" }
      : { total };
  return Response.json(body, { headers: HEADERS });
}
