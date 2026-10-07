-- Download counts for the website (D1 database "secblitz-downloads").
-- Totals per file, added up from Cloudflare's request analytics by
-- functions/api/downloads.js. Nothing about the visitor is stored.
CREATE TABLE IF NOT EXISTS downloads (
  file TEXT PRIMARY KEY,
  count INTEGER NOT NULL DEFAULT 0
);

-- How far the analytics have been added up (milliseconds since 1970), and
-- which refresh did it, so two refreshes at once never add the same hour twice.
CREATE TABLE IF NOT EXISTS progress (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  until INTEGER NOT NULL,
  token TEXT NOT NULL DEFAULT ''
);
