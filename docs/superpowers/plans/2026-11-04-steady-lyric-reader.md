# Steady lyric reader

User reports glitching and losing place when timed lyrics change. Source combines keyed FLIP, entry fly, outgoing fade, role font-size changes, whole-window adaptive resizing and centered flex layout. These compete and can leave outgoing rows in layout while sizing changes.

Replace moving keyed rows with three stable index slots. Current slot remains anchored middle and occupies2.8parts versus1part perneighbor. Each slot independently fits text in its own fixed region; previous18pxcap/32percentopacity,next18pxcap/56percentopacity,current42pxcap/650weight and short quiet alignment mark. Keep wrapping, translated sublines, original fallback, fixed440x210island, no scrollbar/provider footer. One260ms7pxentry fade/slide peridentitychange; cancel prior animation on new identity, no outgoing clone or FLIP. Reduced motion supported. Plain pagination unchanged.

Browser checks forward/backward seeks5times: current slot center129.984375pxunchanged, exactly3rows at all sampled transitions, running transition observed, reducedmotion suppressesanimations, no overflow/pageerrors. Additional unchanged-playing test observed0DOMmeasurementprobes over2.1seconds, verifying font fitting is not repeated on position ticks/metadata polls. Frontend208tests andcheck0errors/0warnings;debugbuildrelaunch. Native sources unchanged, preceding suite65passed/2ignored.
