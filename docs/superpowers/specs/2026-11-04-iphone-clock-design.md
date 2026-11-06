# iPhone Clock connection

Approved in chat: local Wi-Fi connection, one-time iPhone Shortcut setup, one-way Sudo to Clock, preserve other alarms.

The opt-in native service shares a bounded snapshot of upcoming Sudo routine alarms on the private LAN. Pair with a locally generated secret shown only in Settings and a setup QR. No account, cloud, or always-running process when disabled. Windows and iPhone must share a trusted local network; HTTP is local, not Internet exposed. A successful fetch is distinct from a Shortcut import acknowledgement.

Shortcuts is the supported Clock bridge. Provide a complete guided recipe, not an unsigned file advertised as installable. It fetches and validates the manifest before touching alarms, finds only the installation-specific Sudo label prefix, creates one-shot alarms for the next 24 hours, and reports completion. Daily / Wi-Fi automation refreshes the schedule. The phone owns ringing after import. PC edits arrive on the next Shortcut run. Existing non-Sudo alarms, tones, volume, snooze, and sleep schedules stay separate. Fetch failure must preserve existing alarms. No promise of two-way or instantaneous sync.

The service filters on request time, rejects stale snapshots, authenticates every schedule/ack request, accepts private IPv4 clients only, limits HTTP headers/body/timeouts, rejects browser-origin API requests, and exposes no file/config/command access. Disable closes the listener. Settings displays setup, private-network troubleshooting, last fetched and last imported status, and an explicit disable behavior (phone alarms already imported remain until removed in Clock).

Reuse `occurrencesFor` and the scheduler start date, including custom entries, disabled reminders, midnight rollover, and gradual shifts. Publish at startup, schedule changes and every 30 seconds off the animation path. No mutation of local alarm progress.

Verify schedule boundaries and stale/auth/ack/disable behavior in tests, then actual native HTTP and rendered Settings. iPhone import/ringing verification requires the user's phone and remains explicitly unverified until performed.
