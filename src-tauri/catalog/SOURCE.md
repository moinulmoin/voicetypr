# AI provider catalog — source & refresh

- Upstream: https://models.dev/api.json (MIT)
- Fetched: 2026-06-13
- Projected snapshot SHA256: 79627f3b5accfd726da15d1ad13b2220b2b391621c5fd664fb080382b6f0a0f2
- Providers: 3 | text->text models: 94

## Deterministic filter (plan 017, STOP-1 tighter rule)
A provider is in the catalog IFF it has an entry in `overlay.json` mapping it to a
genai 0.6 `AdapterKind` (the runnable-via-genai set). Models are pulled
automatically from the pinned snapshot (all text-in/text-out models per provider).
The full 145-provider api.json (`.cache/`, gitignored) is NOT committed.

## Refresh
1. Re-fetch api.json to `.cache/models.dev.api.full.json`.
2. `python generate.py --refresh` (re-projects overlay providers + used fields -> models.dev.snapshot.json).
3. `python generate.py` (snapshot + overlay -> catalog.generated.json).
4. Review diff, run `cargo test ai`, commit.
Never fetched at app runtime.
