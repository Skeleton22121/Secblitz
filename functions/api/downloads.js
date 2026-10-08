// Totals come from Cloudflare's request analytics rather than from code in
// front of the downloads: the files and the update path stay untouched, and
// downloads served from Cloudflare's cache are seen too.
const ZONES = [
  "67fffa53121a4368f163e075791f2580", // secblitz.lol
  "c5ee798fa38e35a0c49ed5a293b063ee", // beacons.lol (older updaters)
];
const FILE = /^\/downloads\/(secblitz-\d+\.\d+\.\d+-windows-(?:x64|arm64)(?:-setup)?\.exe)$/;
const AUTOMATION = /curl|wget|bot|spider|crawl|python|go-http|java|okhttp|libwww|headless|powershell|secblitz-release|secblitz-owner/i;
const MINUTE = 60 * 1000;
const LAG = 10 * MINUTE; // analytics arrive a few minutes late
const REFRESH = 10 * MINUTE;
const WINDOW = 24 * 60 * MINUTE; // the longest span one analytics query may cover
const KEPT = 30 * 24 * 60 * MINUTE; // how far back analytics can be read
const QUERY = `query ($zone: String!, $from: Time!, $to: Time!) {
  viewer { zones(filter: { zoneTag: $zone }) {
    httpRequestsAdaptiveGroups(limit: 10000, filter: {
      datetime_geq: $from, datetime_lt: $to, requestSource: "eyeball",
      clientRequestPath_like: "/downloads/%", clientRequestHTTPMethodName: "GET",
      edgeResponseStatus: 200
    }) { count dimensions { clientRequestPath userAgent } }
  } }
}`;
const HEADERS = {
  "Cache-Control": "public, max-age=300",
  "Content-Security-Policy": "default-src 'none'; frame-ancestors 'none'",
  "X-Content-Type-Options": "nosniff",
  "Referrer-Policy": "no-referrer",
  "Strict-Transport-Security": "max-age=31536000",
};

// Whole downloads only: a resumed or split download is answered with 206.
async function downloadsBetween(env, from, to) {
  const totals = new Map();
  for (const zone of ZONES) {
    const response = await fetch("https://api.cloudflare.com/client/v4/graphql", {
      method: "POST",
      headers: { Authorization: `Bearer ${env.ANALYTICS_TOKEN}`, "Content-Type": "application/json" },
      body: JSON.stringify({
        query: QUERY,
        variables: { zone, from: new Date(from).toISOString(), to: new Date(to).toISOString() },
      }),
    });
    const result = await response.json();
    if (!response.ok || result.errors) {
      throw new Error(`Analytics query failed: ${JSON.stringify(result.errors)}`);
    }
    for (const group of result.data.viewer.zones[0].httpRequestsAdaptiveGroups) {
      const file = FILE.exec(group.dimensions.clientRequestPath);
      if (file && !AUTOMATION.test(group.dimensions.userAgent)) {
        totals.set(file[1], (totals.get(file[1]) || 0) + group.count);
      }
    }
  }
  return totals;
}

async function refresh(env) {
  const progress = await env.DB.prepare("SELECT until FROM progress WHERE id = 1").first();
  const end = Date.now() - LAG;
  if (!progress || end - progress.until < REFRESH) return;
  const from = Math.max(progress.until, end - KEPT);
  const to = Math.min(end, from + WINDOW);
  const totals = await downloadsBetween(env, from, to);
  const token = crypto.randomUUID();
  const statements = [
    env.DB.prepare("UPDATE progress SET until = ?1, token = ?2 WHERE id = 1 AND until = ?3")
      .bind(to, token, progress.until),
  ];
  for (const [file, count] of totals) {
    statements.push(
      env.DB.prepare(
        "INSERT INTO downloads (file, count) SELECT ?1, ?2 WHERE EXISTS (SELECT 1 FROM progress WHERE id = 1 AND token = ?3) " +
          "ON CONFLICT (file) DO UPDATE SET count = count + excluded.count",
      ).bind(file, count, token),
    );
  }
  await env.DB.batch(statements);
}

function short(n) {
  if (n < 1000) return String(n);
  if (n < 1000000) return `${(n / 1000).toFixed(n < 10000 ? 1 : 0).replace(/\.0$/, "")}k`;
  return `${(n / 1000000).toFixed(1).replace(/\.0$/, "")}M`;
}

// ?format=badge answers in the shields.io endpoint format for the README.
export async function onRequestGet({ request, env, waitUntil }) {
  waitUntil(refresh(env).catch((error) => console.error(error)));
  const row = await env.DB.prepare("SELECT COALESCE(SUM(count), 0) AS total FROM downloads").first();
  const total = Number(row.total);
  const body =
    new URL(request.url).searchParams.get("format") === "badge"
      ? { schemaVersion: 1, label: "downloads", message: short(total), color: "18181B" }
      : { total };
  return Response.json(body, { headers: HEADERS });
}
