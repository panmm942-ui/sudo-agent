# Launch kit

Drafts for announcing Sudo. Everything here is a draft for the maintainer to edit and post personally —
nothing has been posted. Keep claims to what the current release actually does.

---

## Show HN draft

**Title** (≤ 80 chars, no superlatives, HN style):

> Show HN: Sudo – A Dynamic Island for Windows with Spotify lyrics and liquid glass

**Link:** `https://github.com/panmm942-ui/sudo-agent`

**Text:**

> Hi HN, I built Sudo, a small "island" that lives at the top of the Windows desktop. It stays as a
> compact pill and expands on hover into a Home panel (clock, reminders, a focus timer, a note) or a
> Spotify panel.
>
> The parts I spent the most time on:
>
> - **Glass that refracts the real desktop.** The island is a WebGL shader over a live capture of what's
>   behind it (Windows Graphics Capture), not a blurred screenshot. Getting it to animate without dropping
>   frames meant a lot of measuring: springs that integrate real elapsed time, scissored rendering, no
>   layout reads in the draw loop.
> - **Synced lyrics.** Timed lyrics come from Spotify's own client when you enable the optional connector,
>   otherwise from LRCLIB or NetEase. It only uses a record when the duration and metadata match, and it
>   never makes up timestamps for plain lyrics. Word-by-word highlighting and a per-song sync offset are
>   included.
> - **Real audio spectrum.** The bars come from Spotify's process audio (WASAPI process loopback), not a fake animation.
> - **Optional local vocal reduction.** On an NVIDIA GPU it can run an MDX-Net model locally and route only
>   Spotify through VB-CABLE so you can turn vocals down for karaoke. Adds ~1.1–1.5 s of delay. It's
>   strictly opt-in and documented as such.
>
> It's MIT-licensed. Stack: Tauri 2 (Rust) + Svelte 5. The installer is about 5 MB. Playback controls use the Windows media
> session, so no Spotify developer account is needed for the basics.
>
> Honest caveats: Windows 10/11 x64 only, the binaries are not code-signed yet (SmartScreen will warn;
> every release has SHA-256 sums), and it's a personal project I built for my own desk. I used AI coding
> assistants heavily while building it.
>
> I'd love feedback on the glass rendering approach and on what you'd want an island like this to show.

**Before posting**

- Post Tuesday–Thursday, roughly 14:00–16:00 UTC, when you can answer comments for ~3 hours.
- Make sure the latest release is published and `scripts/install.ps1` works from `master`.
- Have the demo GIF at the top of the README (HN readers click through to the repo).
- Don't ask friends to upvote; HN detects vote rings. Reply to every substantive comment.

---

## Short launch description

**One line (GitHub description, ≤ 350 chars):**

> A Dynamic Island for Windows: liquid-glass panels for Spotify (synced lyrics, real audio spectrum,
> animated artwork), reminders, alarms and a focus timer. Tauri + Svelte.

**Paragraph (Reddit, Product Hunt, blog):**

> Sudo is a small island at the top of your Windows desktop. Hover it to control Spotify, read
> synced, word-by-word lyrics, see animated album artwork and a real audio spectrum, or check your next
> reminder and start a focus timer — without switching windows. The glass refracts your actual desktop.
> It's free, about 5 MB, built with Tauri and Svelte, and keeps credentials in Windows Credential Manager.
> Optional extras include an iPhone alarm companion and GPU vocal reduction for karaoke.

---

## Distribution checklist

Repository readiness
- [x] License chosen (MIT) and `LICENSE` committed; README badge and `app/package.json` match
- [ ] Demo GIF at the top of the README; screenshots current
- [ ] Repository description, website and topics set (see the final report / README)
- [ ] Social preview image uploaded (**Settings → General → Social preview**, `docs/images/social-preview.png`)
- [ ] CI green on `master`; latest release published with installer, portable exe and `SHA256SUMS.txt`
- [ ] One-command install tested from `master` on a clean Windows user account
- [ ] Private vulnerability reporting enabled (**Settings → Code security**)
- [ ] Labels created (`docs/MAINTAINING.md`), a few `good first issue` issues filed
- [ ] Discussions enabled (optional) for questions and show-and-tell

Launch channels (stagger over 1–2 weeks; reuse what you learn)
- [ ] Show HN (draft above)
- [ ] Reddit: r/Windows11, r/windows, r/Rainmeter (desktop customisation), r/truespotify, r/rust (Tauri angle), r/sveltejs — read each sub's self-promotion rules first
- [ ] Tauri Discord `#showcase` and the [awesome-tauri](https://github.com/tauri-apps/awesome-tauri) list (PR)
- [ ] Svelte Discord / Svelte Society showcase
- [ ] Product Hunt (prepare 3–5 gallery images at 1270×760 and the GIF)
- [ ] winget package (`winget-pkgs` manifest PR) once releases are stable — needs a consistent installer URL, ideally code signing
- [ ] Short screen recording for X/Bluesky/Mastodon (#tauri, #svelte, #windows11)

After launch
- [ ] Triage new issues daily for the first week
- [ ] Pin a "Roadmap / known limitations" issue
- [ ] Ship a quick patch release for the top reported bug to show the project is alive
