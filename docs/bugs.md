# Known Issues, Edge Cases, Sharp Corners

## Active Bugs
- **First-Load Black Map Bug:** 
  - *Impact:* GoldSrc fails to render lighting on the first demo load of a session. 
  - *Workaround:* Implement a "Primer Demo" strategy. Duplicate the first demo, strip it, and use it strictly to pre-cache map assets before daisy-chaining to the actual target.
- **The Quit Filter Bypass:** 
  - *Impact:* GoldSrc actively drops any `ConsoleCommand` containing the string `quit` during playback, preventing automated batch termination. 
  - *Workaround:* Map `+alias dodtools_exit quit` in launch arguments and inject `dodtools_exit` into the `.dem` stream.

## Edge Cases
- **SZ_GetSpace Overflow:** Fast-forwarding a demo at high speeds causes the engine's UI text buffer to overfill, throwing console errors. This is harmless and naturally clears when reverting to `host_framerate 0`.
