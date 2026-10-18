[中文](../zh-CN/codex-radar.md) | [Back to English README](../../README_EN.md)

# CodexRadar recommendations

CodexRadar is an optional, experimental enhancement. Monitor uses public RadarBench scores to calculate non-personalized recommendations for model and reasoning-effort combinations. It is disabled by default and does not affect core usage monitoring.

## Enable and use it

Choose **Settings > CodexRadar > Enable CodexRadar**. The first enable action presents a privacy notice, and no CodexRadar request is sent until you confirm it.

After enabling it, pause over the Widget content for about 500 ms to see recommendations. The left drag handle does not trigger the tooltip. You can also open the site from the menu or by triple-clicking the Widget content.

## Recommendation display

The tooltip shows Monitor's computed daily and hard-problem recommendations. The application no longer requests or displays the site's own recommendations, so failures of that endpoint do not affect Monitor's refresh status.

| Label | Selection method |
| --- | --- |
| Daily | Uses current public RadarBench summaries with all 64 tasks covered, Radar score ≥ 90, and valid cost and duration, then minimizes `relative price ^ 0.7 × relative time ^ 0.3`; Radar score breaks cost ties |
| Hard problem | Keeps those combinations with mean API-equivalent cost ≤ $9 and duration ≤ 30 minutes; targets $8 and 30 minutes, choosing the higher Radar score when the gap reaches 10 points, otherwise choosing the lower daily cost |

The application first reads the site's current public model and effort binding, then fetches public score summaries with at most three concurrent requests. Radar scores use the server's 0–150 results directly, without deriving a pass rate or applying the legacy IQ Wilson bound. Cost and duration are means of their known samples; missing values are not treated as zero. API-equivalent cost is not a subscription charge or an actual payment. Combinations without all 64 tasks covered or valid cost and duration are excluded.

If Monitor's current score API is unavailable, it falls back to the legacy raw efficiency endpoint. Legacy data displays IQ and uses the 95% Wilson lower bound as credible IQ for daily and hard-problem ranking. Archived static IQ snapshots are not used as current data. These results come from community data and are not filtered against models available to your account. A recommended combination might therefore be unavailable to you.

## Refresh, cache, and failures

- After Monitor recommendations refresh successfully, data refreshes about once per hour; manual refresh has a 60-second cooldown
- Only Monitor's derived recommendations, the score basis, cache times, and request validators are stored; complete responses are not retained verbatim. Failed configuration reads preserve valid cached data and schedule a retry
- Upgrades migrate the old cache, keeping unexpired Monitor recommendations, dropping site recommendations, and clearing the old failure backoff before an immediate startup refresh
- Cached data is kept for up to 24 hours; failed refreshes mark it as potentially stale and use progressively longer retry intervals
- CodexRadar failures, stale data, or incompatible response formats do not stop core Codex usage polling

The application does not send your Codex Token, usage data, or project content to CodexRadar. See [Diagnostics and privacy](diagnostics-and-privacy.md) for the complete network-access summary.
