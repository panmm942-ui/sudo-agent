# Your online iPhone companion

Sudo supports an optional single-person HTTPS companion hosted in your own Cloudflare account. The phone can access it on mobile data or any Wi-Fi. It does not expose your PC or require a router port or local tunnel.

## Connect your phone

1. Open **Settings → Alarm → iPhone Clock → Online iPhone companion** in the Windows app.
2. On this configured PC, choose **Pair iPhone**. Scan the private QR with Camera and open it in Safari. The pairing code expires after ten minutes and can be used once. Claiming a new code replaces the previous phone capability.
3. Bookmark the companion, or add it to your Home Screen. That browser keeps its phone key locally. Do not share the pairing link or key.
4. For native Clock alarms, open **Connection & iPhone setup → Set up iPhone Clock** on the phone. Follow the guided **manual** Shortcuts recipe. Set daily refresh automation and run it after schedule edits. Nothing imports into Clock just by opening the website.

## Features

- **Clock alarm feed:** the PC uploads seven days of its real schedule, including weekdays, enabled entries, reminder overrides and gradual time shifts. Every phone Shortcut run imports the next 24 hours as one-shot Clock alarms. It creates new alarms before deleting the captured previous set, and only manages the exact Sudo ownership prefix. Clock sounds, volume, Sleep schedules and snooze remain separate.
- **Stored coverage:** the phone can fetch the remaining stored schedule while the PC sleeps. The last PC contact, source-update time and coverage end are shown. The feed refuses insufficient/expired coverage; it does not return a misleading empty schedule. Bring the PC online before coverage runs out. Freshness metadata describes the feed response separately from the underlying PC upload.
- **Shared note:** the companion’s note is the Home Personal note widget. Phone and PC changes sync while connected. Polling preserves an unsaved browser draft; in-flight desktop edits wait for the next sync. First connection seeds an empty service from existing Home data. Later simultaneous edits use timestamps; keep PC and phone clocks correct.
- **Focus timer:** change duration, start, pause or reset from the phone or PC. Both use an absolute deadline so counting down does not require constant network messages. The phone page does not promise a background alarm or a notification when Safari is suspended.
- **Send to Sudo:** send text and links to the bounded inbox shown in PC Settings. Copy or dismiss them there. Sudo never automatically opens a link or executes received text. A Share Sheet Shortcut recipe is included on the phone page.
- **Optional battery reports:** a separate phone Shortcut can report percentage and charging status. Settings shows the timestamp. This is the last report, not continuous background phone access. No location, microphone, camera, messages or contacts are collected.

The PC syncs every fifteen seconds while configured, and after local changes. The phone refreshes visible pages every fifteen seconds. There is no instantaneous push into Clock, no two-way Clock-database sync, and no remote iPhone control. Imported Clock alarms can ring independently of both the PC and service after a successful import. Actual phone permissions, import, automation and ringing need to be checked on the real device.

**Revoke phone access** immediately blocks its capability and pending pairing code. **Forget this browser** only removes that browser’s saved copy. Disconnecting the PC stops PC sync and removes the desktop credential locally; it does not delete the deployed service, stored cloud data or existing Clock alarms. Revoke first if you also want to block the phone. If migrating from local Wi-Fi imports, remove the old local connection’s labelled alarms in Clock to avoid retaining two separate Sudo sets.

## Deploy your own service

The `companion/` directory contains the Worker, SQLite Durable Object and static phone UI. It uses no third-party frontend framework. Cloudflare plan quotas apply; no paid plan is required by this code, and limits should be checked in your account.

```powershell
cd F:\sudo\companion
npx wrangler@4 login
npx wrangler@4 deploy
npx wrangler@4 secret put DESKTOP_KEY
```

Use a unique random **64-character lowercase hexadecimal** desktop key, entered through Wrangler’s secret prompt. Keep it out of Git, command arguments, logs and screenshots. In Sudo Settings enter the resulting `https://…workers.dev` origin and desktop key once. Windows Credential Manager stores the key; the PC config file stores only the origin. The browser receives a separate revocable phone key, and the service stores its hash. Deployment uses the existing Cloudflare account authentication; do not send passwords or API keys through chat.

Authenticated API routes are exact and bounded. Native requests are island-only, HTTPS-only, restricted to the configured workers.dev origin, time-limited and reject redirects. Browser API requests accept only the service’s own origin. Responses are `no-store`, assets have a restrictive content security policy, and the app loads no third-party scripts. The service stores schedule, note/focus state, inbox and optional battery report in your account; this is access-controlled cloud storage, not end-to-end encryption.

## Verification boundary

Automated tests cover pairing expiry/reuse/revocation and roles, schedule dates/offsets/coverage, atomic actions, privacy of public state, note/focus synchronization and the video flicker regression. Native app/browser integration can verify the HTTPS round trip. Those checks do not prove iPhone Clock ringing or that iOS will execute a particular automation unattended. Test those manually before relying on alarms.
