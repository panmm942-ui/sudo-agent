# Optional local vocal reduction

Approved: opt-in GPU processing with approximately 1.7 seconds delay and a virtual audio cable. Keep Sudo's glass, layouts and normal idle behavior unchanged.

1. Verify GPU throughput and per-process Windows route round trip using synthetic audio / our own process. Done: 0.224s per 1.6s window; route round trip restored.
2. Test bounded audio queues, continuous sample ordering, blend endpoints, cable-only capture and route backup/restore. Done: six synthetic tests.
3. Add a native controller with explicit start/stop, status, bounded shutdown and crash recovery. No model load or Python process on ordinary startup unless an interrupted route backup needs recovery.
4. Add Settings > Spotify controls: setup readiness, physical output, vocal level, optional enable switch and processing status. Persist level only; never auto-enable.
5. Prepare official signed VB-CABLE driver. User performs elevated installation/reboot; do not install silently or change Windows' default endpoint.
6. Check frontend/native suites, rendered settings interactions, model smoke and normal launch. Record the missing driver / end-to-end audio validation separately.
