# Home pin, border and lyric wrap polish

User specification: Home click pins/unpins just like Spotify. Replace Spotify pin dot with subtle light around artwork. Darken both expanded and compact Home. Add optional white line instead of glass around island. Remove settings gear's circular background. Wrap lyric lines onto subsequent lines.

Implementation: independent per-page pin states, transparent accessible Home click surface, artwork aria-pressed glow. Smoothly darken Home material. Persist islandBorder=glass|white-line; shader white-line mode bypasses refraction/specular and draws flat dark fill with antialiased white contour. Glass stays default. Settings gear stays same position, transparent hover/background. Lyrics paragraphs wrap; preserve fixed440x210size and three logical lyric rows, allow keyboard scrolling only when the combined content exceeds the available area; keep current row visible.

Validation: preference migration regression first, browser pin/unpin/page independence, artwork glow without dot, unframed gear, wrapped current lyric height, fixed dimensions, outline rendering/persistence, dark Home expanded/compact. Existing full suites/check/build and source review. Preserve prior dirty work; no commits/staging.

Verified:190frontendtests pass;0frontenderrors/warnings. Browser:Homepin/unpin and independent retained pins acrosspages, no Spotifydot, computedartworkglow, transparentgear, savedwhiteoutline. Long currentlyric72pxhigh at24pxlineheight with nohorizontaloverflow; playerbefore/after440x210. Screenshots inspected:darkHome,compactHome,whiteoutline,pinglow,wrappedlyrics. No browser/GL errors. Independent source review:no blocking/P2findings.
