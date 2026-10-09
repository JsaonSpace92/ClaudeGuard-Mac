# Dashboard design QA

final result: passed

Date: 2026-10-07. Scope: visual and interaction acceptance of the revised existing ClaudeGuard dashboard. Network protection acceptance is separately documented in `docs/DASHBOARD-VALIDATION.md` and is not complete.

## Evidence

- Reference: 本机验收附件（不公开临时文件路径）.
- Final implementation: 本机验收附件（不公开临时文件路径）.
- Side-by-side comparison: 本机验收附件（不公开临时文件路径）; inspected together after the final update.
- Narrow layout: 本机验收附件（不公开临时文件路径）.
- Risk and IPv6 masking state: 本机验收附件（不公开临时文件路径）.
- Preview: `http://127.0.0.1:8796/`; fixture data only. The preview bridge is outside the application source and packaged App.

## Visual checks and adjustments

The desktop layout was exercised at 980 × 860 CSS pixels. The screenshot is 965 × 847 raster pixels; the reference application frame was cropped to 1182 × 1062 and proportionally resized for comparison. Operating-system window controls and reference desktop wallpaper are excluded from implementation scope, since Tauri supplies native window chrome.

The initial layout clipped the lower environment rows. Card spacing and row heights were reduced to accommodate the extra IPv6 and UDP checks. Final DOM measurements put the location row bottom at 793.77 and the fixed footer top at 796, with all overview rows visible and no horizontal overflow. The 520 × 640 layout was tested in an exact-size frame using the same UI files; major footer controls remained accessible. This is a responsive layout test, not a claim of resizing the native running App.

The final comparison retains the reference hierarchy: orange confirmation status, IP visibility switch, two exit cards, network checklist, system environment checklist, and persistent action footer. The dark background (#202020), subtle borders (#3a3a3a), blue action (#0a84ff), orange confirmation (#ff9b33), and green success (#31c45f) maintain the supplied visual direction. Native system typography uses SF/PingFang fallbacks. Icons come from the Phosphor library, with its MIT license retained; no custom raster assets or fabricated window controls are needed.

Intentional functional differences: dynamic system/proxy exit labels; separate IPv6 and UDP rows; expandable scopes and evidence; unknown results remain neutral; launch obeys existing guard requirements; IP reputation uses an actual public list and does not imply a Claude risk score. Country labels derive from results rather than hardcoded flags. Settings and original guard controls remain available below the overview.

## Interaction checks

The actual frontend was exercised with an isolated preview bridge: IP masking, detail expansion, repeated checks, browser-result polling, old-result invalidation after saving, IPv6 result masking, guard conditions, and stopping despite invalid form values. Browser console warnings/errors were empty at final capture. Separately, the real browser session exercised DNS sampling and report return. Build and native test evidence appears in the validation report.

No unresolved P0, P1, or P2 visual/interaction issue was observed in the inspected states. System blocking, browser extension installation, real target-app flow coverage, and user login-item changes have not received end-to-end acceptance and are explicitly excluded from this visual pass.
