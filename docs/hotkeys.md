# Hold-to-talk triggers

Default: hold **Right Ctrl** ≥ 180 ms (alone — any other key or click cancels).

Add more triggers in `%APPDATA%\dev.sudo.island\hotkeys.json` (read at startup — restart sudo after editing):

```json
{ "hold": ["RControl", "F13"], "swallow": ["F13"] }
```

- `hold` — names are case-insensitive: `RControl`, `F1`–`F24`, `MiddleButton`, `XButton1`, `XButton2`, media/volume key names, or a raw virtual-key code `vk:0x87`. Left/right mouse buttons are refused. Missing/invalid file → Right Ctrl only.
- `swallow` — triggers (other than Right Ctrl) whose presses are hidden from other apps. Use it for a key that exists only to talk to sudo.

## Mouse button that Windows can't see (e.g. a DPI/sensitivity button)

1. Open the mouse's own app (Logitech G Hub, Razer Synapse, SteelSeries GG, Corsair iCUE, …).
2. Assign the button to the keyboard key **F13** (no keyboard has it, so nothing else reacts).
3. Make sure the assignment **holds the key while the button is held** — not a single tap. (G Hub: "Keystroke"; Synapse: "Keyboard function"; both hold by default.)
4. Put `"F13"` in `hold` (and `swallow`) as above, then restart sudo.
