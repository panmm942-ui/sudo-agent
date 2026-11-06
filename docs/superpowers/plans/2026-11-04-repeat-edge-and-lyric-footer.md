# Repeated top-edge gestures and lyrics footer

User requests no Lyrics/LRCLIB label and reliable top-edge avoidance while Home/Spotify/lyrics are pinned, including compact mode after other interactions.

Root causes: frontend explicitly disabled native edge avoidance in lyrics; native gesture latch stayed disarmed after return until pointer exited whole expanded corridor. Remove lyrics prohibition, use derived scalar eligibility to avoid redundant enable commands, rearm on reaching lowered island because pointer has already left activation strip. Holding at top still cannot cause repeated activations. Keep fixed44px drop and existing pin/hover semantics. Restore regular native window after leaving avoidance even while lyrics remain open.

Remove provider footer on timed lyrics, including empty row reservation; retain only meaningful language errors and unsynced pagination. Existing adaptive fit uses newly freed space.

Validation: simulated bridge cycles for pinnedSpotify, pinnedlyrics twice, pinnedHome, compactafterSettings all y10→54→10; native eligibilitytrue duringlyrics; nofooter. Native tests cover fresh top visit without corridor exit and disable/re-enable forcompact. Frontend208pass, native65pass/2ignored,check0errors/0warnings; build/relaunch.
