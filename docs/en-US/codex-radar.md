[中文](../zh-CN/codex-radar.md) | [Back to English README](../../README_EN.md)

# CodexRadar recommendations

CodexRadar is an optional, experimental enhancement that shows non-personalized community recommendations for model and reasoning-effort combinations. It is disabled by default and does not affect core usage monitoring.

## Enable and use it

Choose **Settings > CodexRadar > Enable CodexRadar**. The first enable action presents a privacy notice, and no CodexRadar request is sent until you confirm it.

After enabling it, pause over the Widget content for about 500 ms to see recommendations. The left drag handle does not trigger the tooltip. You can also open the site from the menu or by triple-clicking the Widget content.

![CodexRadar recommendation tooltip](../images/codex-radar-recommendation.png)

## Recommendation display

| Label | Selection method |
| --- | --- |
| Speed / Smart | Shows explicit `speed` / `smart` slots when provided; the legacy `value` recommendation is kept as Speed |
| Community daily | Shows an untagged `daily_development` list in the supplied order without inferring position semantics |
| Daily | Uses the raw efficiency data to keep combinations with IQ ≥ 90 and valid price and duration, then minimizes `relative price ^ 0.7 × relative time ^ 0.3`; credible IQ breaks cost ties |
| IQ-first | Uses the same candidates and selects by `0.8 × relative IQ score + 0.2 × relative price score`, favoring IQ |

Daily uses task pass rate, average price, and average duration from CodexRadar's raw efficiency endpoint. The tooltip shows raw IQ, while ranking uses the 95% Wilson lower bound as credible IQ to reduce small-sample noise. These results come from community data and are not filtered against models available to your account. A recommended combination might therefore be unavailable to you.

## Refresh, cache, and failures

- After a complete success, data refreshes about once per hour; manual refresh has a 60-second cooldown
- The two recommendation sources are cached and validated independently; complete responses are not retained verbatim
- Cached data is kept for up to 24 hours; failed refreshes mark it as potentially stale and use progressively longer retry intervals
- CodexRadar failures, stale data, or incompatible response formats do not stop core Codex usage polling

The application does not send your Codex Token, usage data, or project content to CodexRadar. See [Diagnostics and privacy](diagnostics-and-privacy.md) for the complete network-access summary.
