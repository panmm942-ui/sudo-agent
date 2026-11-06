# Island polish and startup

Approved in chat: retain liquid glass and the existing right-hand Home/Spotify rail and gear positions. Keep the centered Spotify player as default. Offer larger artwork left, title upper right and controls lower right as a preference. Redesign the settings content with readable spacing and discreet scrollbars while retaining the glass shell.

Protect Home after returning from Spotify: ignore residual upward scroll until a quiet interval, then require accumulated deliberate scrolling to hide. Remove Home's alarm test and keep the full test in Alarm settings. Every song gets a restrained cover-derived tint; monochrome art gets graphite, missing art a neutral dark fallback, never a random hue. Home may optionally share the cover atmosphere and image. Smooth material transitions rather than resetting tint on page changes. Improve compact waveform compositing without continuous JavaScript animation.

Add optional Windows current-user sign-in startup, off unless explicitly enabled. Read the actual OS setting, show failures, and avoid another service/dependency. Explain that Spotify's native music video isn't supplied by the current media-session integration; local linked videos remain supported with a useful playback failure state.

Approach: extend existing Svelte/Tauri implementation and tokens. A separate settings window or replacing the glass surface would violate the approved constraints. No remote artwork/video services or new frontend dependencies. Optimize redundant DOM/state/GPU work while preserving appearance and behavior.

Motion: material RGB and tint blend interpolate over approximately 300 ms; content keeps existing transitions; waveform uses compositor transforms and honors reduced motion. Verify regression behavior, rendered screens, keyboard, reduced motion, native compilation/launch, and measure performance without promising unmeasured savings.
