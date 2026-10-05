# Web protection: block ads, tracking and dangerous websites

Status: draft for owner review. Target release: 0.8.0.

## Goal

A system-wide DNS filter, safe for people with no technical knowledge. Three
independent switches:

- **Block ads**
- **Block tracking and telemetry** (websites, apps and Windows/Office telemetry)
- **Block dangerous websites** (phishing and malware)

All three are off after install. Secblitz suggests them; the person switches
them on. Any combination works, and all three off means no filter at all.

Decisions already made by the owner: local filter (not hosts file, not a public
filtering DNS); lists from AdGuard's registry; off by default but suggested;
the uninstall "put everything back" choice also turns this off.

## What a person sees

A new sidebar page, **Web protection**, between Clean up apps and Tools:

- Three switches, each with one plain sentence:
  - Block ads: "Stops ads from loading in your browser and in apps."
  - Block tracking and telemetry: "Stops websites, apps and Windows from sending data about what you do."
  - Block dangerous websites: "Stops your PC from opening known scam and virus websites."
- A status line: "On", "Paused until 3:15 PM", "Getting block lists ready" (first
  start, before any list is downloaded) or "Not working right now. Your internet
  still works, but nothing is being blocked." (filter down, fallback in use).
- **Pause for 1 hour** (with "Resume now" while paused), for when a website
  doesn't work properly.
- "Blocked today: 1,204 ads, 388 trackers, 0 dangerous websites."
- One honest line: "Some ads, like the ones inside YouTube videos, come from
  the same place as the video and can't be blocked this way."

Home shows one optional suggestion card ("Block ads, trackers and dangerous
websites") that opens the page. It is never counted in the score.

## How it works

```
apps and browsers
  -> Windows DNS client
     -> NRPT rule for every name (".") -> 127.0.0.1 / ::1, last resort 9.9.9.9
        -> SecblitzFilter service (LocalService)
             blocked?  -> answers 0.0.0.0 / :: itself
             allowed?  -> forwards the query unchanged to the PC's normal DNS servers
```

### Routing: one NRPT rule, adapters untouched

Windows' Name Resolution Policy Table (NRPT) sends all lookups to the filter
through one rule for the namespace ".". The network adapters keep their own DNS
settings, so:

- the filter forwards to whatever DNS the current network provides (home
  router, office, hotel Wi-Fi sign-in pages all keep working);
- a new network or adapter needs no reconfiguration;
- VPNs keep working, because their more specific NRPT rules win;
- Chrome and Edge fall back to the Windows resolver when NRPT rules exist.

The rule lists three servers in order: `127.0.0.1`, `::1`, then Quad9
`9.9.9.9`. If the filter stops answering, Windows fails over to Quad9 after
about a second and the internet keeps working (unfiltered) until the filter is
back. The rule carries a fixed Secblitz name, and Secblitz only ever removes
its own rule.

The elevated app adds the rule when the first switch goes on and removes it
when all are off. The hourly SYSTEM update task removes it when the filter
service is missing or stopped. The service gets restart-on-failure actions.

**Spike first (VM).** Before building the rest: confirm the "." NRPT rule
routes lookups from Edge, Chrome, Firefox, Store apps and Windows services
to a loopback listener; confirm failover to the third server when loopback is
silent; confirm removal takes effect immediately. If NRPT fails any of these,
fall back to per-adapter DNS (`127.0.0.1` primary, the adapter's own DHCP DNS
read from the registry as the upstream), and revise this spec.

### SecblitzFilter service

- A second Windows service, separate from SecblitzMonitor, running as
  `NT AUTHORITY\LocalService` with only `SeChangeNotifyPrivilege`, like the
  monitor. It doesn't need admin rights: binding port 53 on loopback, reading
  adapter DNS servers and downloading lists are all allowed for LocalService.
- Listens on UDP and TCP `127.0.0.1:53` and `[::1]:53` only. Never on other
  addresses. Drops packets whose source is not loopback.
- std threads and sockets, no async runtime, no DNS crate. The DNS code reads
  only the header and the question, and builds only a small set of
  answers, all hand-written and unit-tested.
- Blocked name: `A` gets `0.0.0.0`, `AAAA` gets `::`, every other type gets an
  empty NOERROR answer (this also covers HTTPS/SVCB records). TTL 60 s.
- Allowed name: the original packet goes to the upstream servers over UDP with a
  fresh random ID and source port. TCP is used on truncation. A reply is
  accepted only if its ID and question match. No cache: Windows already caches.
- Upstream servers: the DNS servers of the active, non-loopback adapters, read
  with `GetAdaptersAddresses` every 30 s and after a failure. If none are
  found, Quad9.
- `use-application-dns.net` gets NXDOMAIN. This is Firefox's standard signal to
  not switch on its own encrypted DNS, which would bypass the filter.
- Lookups are a binary search over sorted 64-bit hashes of every suffix of the
  name (`a.b.example.com`, `b.example.com`, `example.com`). Memory stays small
  (8 bytes per domain: about 23 MB for all three switches), and a lookup takes microseconds.
- Lists are rebuilt on a background thread and swapped in atomically, so lookups
  never wait.

### Lists

Downloaded by the service once a day, over HTTPS from a fixed set of URLs
compiled into the binary. Redirects are refused and each file is size-capped
(16 MiB). The last good compiled set is kept on disk, so the filter works
offline and after a restart. All these lists are GPL or similar; Secblitz
downloads them and never ships them.

| Switch | Built from |
|---|---|
| Ads | AdGuard DNS filter (registry id 1), minus every domain classified as tracking |
| Tracking and telemetry | AdGuard DNS filter entries that also appear in AdGuard Tracking Protection or EasyPrivacy, plus HaGeZi's Windows/Office Tracker Blocklist (id 63) |
| Dangerous websites | HaGeZi's Threat Intelligence Feeds (id 44): phishing, malware, scam and cryptojacking domains |

The AdGuard DNS filter is one merged list of ads and trackers (about 178,000
domains, no markers saying which is which). Classifying entries with AdGuard's
own tracking lists keeps AdGuard's DNS-specific curation and exclusions for
both switches. HaGeZi's Threat Intelligence Feeds (owner's choice) hold about
2.5 million domains: a 13 MB compressed download (52 MB unpacked), about 20 MB
of memory as 64-bit hashes, and under a second to rebuild. To spare metered
and mobile connections, no list is refreshed while Windows reports the
connection as metered; the last good set stays in use.

Parser: accepts only `||domain^` (optionally `$important`) and `@@||domain^`
exceptions, valid hostnames with at least two labels. Everything else is
ignored. Exceptions apply within their own list.

**Never blocked**, whatever the lists say (compiled in, wins over everything):
Windows Update and delivery optimisation, Windows activation, Microsoft's
connectivity check (`msftconnecttest.com`, `msftncsi.com`), Microsoft Defender
updates and cloud protection, Microsoft Store downloads, `secblitz.lol` and
`beacons.lol` (Secblitz updates), and the winget CDN.

### Settings and status files

- `C:\ProgramData\Secblitz\Filter\config.json`: the three switches and
  `paused_until`. Only administrators can write it (the elevated app does);
  the service reads it every 2 s. No other channel exists between the app and
  the service, matching the monitor.
- `Filter\status.json`, written by the service and read by the app: running,
  list dates, counts for today per switch, and the last error in plain form.
- Same protected-directory pattern and DACLs as `Monitor\` and `Status\`.

## Install, upgrade, uninstall

- The installer registers SecblitzFilter (disabled) alongside the other
  services. It is started only when a switch goes on.
- Upgrade stops the filter while files are replaced and starts it again
  afterwards, like ResumeMonitor. The NRPT failover covers the gap.
- Uninstall, from the app or from Windows Settings, with either choice: remove
  the NRPT rule first, then stop and delete the service, then delete `Filter\`.
  Blocking is a Secblitz feature, not a Windows setting, so "keep my PC as it
  is" still removes it.

## Security

- Least privilege (LocalService). Loopback-only listener. Untrusted input
  (DNS packets, list files) is parsed by small bounded Rust code. The filter
  never executes anything.
- Spoofing: random upstream IDs and ports, replies checked against the question.
- Lists come over HTTPS from compiled-in hosts only, are size-capped, and a
  parse failure keeps the last good set.
- Only administrators can change the switches. Unelevated programs can read the
  counts but not change anything.
- Not in scope: browsers with their own encrypted DNS set by the person bypass
  the filter. The page doesn't claim otherwise.

## Testing

- **Unit tests (Linux):** packet parsing and building (including malformed,
  truncated and oversized packets), suffix matching, list parsing, the category
  split, the never-block list, config and pause logic.
- **Windows tests:** service install, start, stop and delete; loopback-only
  binding.
- **VM verification, kept non-disruptive:**
  - `Resolve-DnsName` for known ad, tracker, telemetry and test-phishing
    domains, taken from the downloaded lists at test time, with each switch
    on and off;
  - never-blocked domains still resolve;
  - Windows Update scan, Defender signature update and Microsoft Store all
    still work;
  - Edge on an ad-block test page, compared before and after;
  - the Firefox canary domain;
  - pause and resume;
  - stopping the service: the internet keeps working through the failover, and
    the status line says so;
  - uninstall with both choices leaves no NRPT rule and no service;
  - memory and lookup time within budget (under 60 MB and 1 ms).

## Not in v1

Cosmetic filtering, ads inside YouTube videos, per-site allow lists, custom
lists, encrypted upstream DNS, and long-term statistics.
