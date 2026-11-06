# Sudo app icon

`source.png` preserves the original user-selected Minimal Silver Capsule image. `source.svg` is the clean desktop icon: a silver capsule ring with a transparent exterior and interior, without the original black tile. A vector source keeps the edges crisp at small sizes.

Generate into a temporary directory with `npx tauri icon src-tauri/icons/source.svg --output <temporary-directory>`, then copy the desktop PNG, ICO, ICNS and Windows logo files into this directory. The app favicon uses the generated 32×32 PNG. Mobile-only assets are not required for this Windows app.
