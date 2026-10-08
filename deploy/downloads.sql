-- Download count for the website (D1 database "secblitz-downloads").
-- The last total of GitHub release downloads, read by functions/api/downloads.js,
-- and when it was last asked for (milliseconds since 1970). Nothing about the
-- visitor is stored.
CREATE TABLE IF NOT EXISTS release_downloads (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  total INTEGER NOT NULL DEFAULT 0,
  checked INTEGER NOT NULL DEFAULT 0
);
INSERT OR IGNORE INTO release_downloads (id, total, checked) VALUES (1, 0, 0);
