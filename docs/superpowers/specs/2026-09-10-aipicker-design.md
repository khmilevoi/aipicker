# AI Picker — согласованный проект

Windows desktop widget written entirely in Rust. The user approved the proposed design and requested autonomous completion on 2026-09-10.

## Interaction

A tray icon opens/focuses a compact 420×148 logical-pixel window. Closing hides it; only Exit terminates. The latest user reference is a single model/reasoning line above a wide rounded slider, dots for variants and a white thumb. Use a white background with soft lilac accents. Compact mode has no sorting, metric, price or source controls: only model selection, filter/expand icons, close and discreet status/attribution. The filter opens a taller 420×590 view. Expand to a resizable large view, clamped to the current monitor, with map, benchmark charts and data/balance settings. Filters persist and can include/exclude individual models and OpenAI/Anthropic providers. Selection is informational; it does not modify Codex/Claude configuration or invoke models.

The compact slider combines all available relevant metrics: AA Intelligence, Coding, Agentic and input price, output price, AA cost per task. Convert each to percentile rank against the entire downloaded pool before filtering; average quality ranks and inverse cost ranks separately. Default score is 100 × (0.65 × quality + 0.35 × affordability); the quality weight is adjustable only in expanded settings. Missing measurements are omitted and coverage is visible in details/tooltip. Without either group the score is absent. This is an application-relative balance score, not an official benchmark. Slider order is ascending balance with missing scores first. Advanced mode retains individual price/quality sorting and metric controls.

## Data

Use Artificial Analysis's current free endpoint `/api/v2/language/models/free`, with the user's own key. Download all pages off the UI thread, verify pagination and index version, and replace the cache only after complete success. Keep distinct model IDs, versions and reasoning settings as published. Restrict the initial pool to OpenAI and Anthropic. Null values are missing, never zero. Price is USD per million tokens: input, output, or a transparent weighted blend (default 75% input / 25% output). Quality uses the selected named AA intelligence, coding or agentic index, not a percentage claim. Models missing either plotted value stay available in the selector but are omitted from scatter coordinates with an explicit count.

Update on startup when cache is older than 24 hours, every 24 hours thereafter, and manually. Respect 429 cooldown. On network/auth/schema errors retain the last valid cache and show a useful error. Persist timestamp, index version, filters, selected model and price choice. Store the optional API key with Windows DPAPI; never bundle or log it. Without a key/cache show setup, not fabricated benchmark results. Provide an explicitly labelled synthetic demo for offline UI exploration and testing; demo values never become live cache.

Free API headline indices support native charts. Individual benchmark breakdowns unavailable in the free API must be explained in the UI. SWE-bench is an optional future source, not necessary for the accepted AA index charts. Attribute Artificial Analysis visibly and link to methodology. Free data is for personal/internal use; do not redistribute downloaded data.

## Additional approved requirement: reasoning consolidation

Default-on separate filter simplifies only the compact slider. Within the same provider and exact named model version, recognized reasoning suffixes may be consolidated when a retained cheaper variant loses at most 2 points in each available quality index and saves at least 20% of AA cost per task. Thresholds are configurable and persisted. Do not use token rates as a proxy for reasoning consumption. Missing task costs/scores or unrecognized suffixes preserve variants. No transitive replacements: each hidden variant must point directly to a retained qualifying alternative. Explain replacements and show all manually enabled variants in the expanded picker/charts. Add AA cost-per-task as an optional price axis with correct units.

## Components and validation

`domain.rs`: model validation, prices, metric selection, stable sorting and filters. `source.rs`: HTTP, pagination, response normalization and failures. `storage.rs`: atomic settings/cache and DPAPI credentials. `app.rs` / `charts.rs`: UI and selection. `tray.rs`: tray lifecycle, wakeups, positioning and explicit exit. `main.rs`: startup and command options.

Verify model math, missing values, filtering, pagination, authentication errors and cache preservation using real parsers and a local HTTP fixture server. Build and lint the Windows executable. Exercise actual window open/hide/reopen/exit, slider/filter/chart interactions and persistence in a Windows desktop smoke test. Package a release executable plus Russian usage instructions. Live authenticated AA success requires the user's own key; validate the wire schema against the official OpenAPI and test unauthenticated service reachability without inventing credentials.
