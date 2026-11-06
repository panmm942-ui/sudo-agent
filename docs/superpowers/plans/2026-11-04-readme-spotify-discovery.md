# Visual README and Spotify discovery

User requests a visual GitHub README covering the design and features, and asks why Spotify is missing from the released application.

- Create a source-audited README with real UI captures, download/setup instructions and explicit optional/experimental feature boundaries.
- Keep Home and Spotify navigation visible regardless of song availability. Provide an empty Spotify screen with fixed-URI Open Spotify and explicit Retry.
- Preserve the last track through brief read failures, clear stale controls after bounded retries, and report native session discovery failures without exposing account/song data in logs.
- Initialize and balance WinRT on the worker performing each native operation. Confirm native session data reaches the real optimized frontend; do not claim an exact original root cause that was not reproduced.
- Validate rendered README, local links, no-session/recovery UI, frontend/native checks and optimized startup. Publish the source and README to the default branch by fast-forward only.
- Package a v0.1.1 patch release, retaining v0.1.0 and recording verified artifacts and commit identity in the delivery handoff.
