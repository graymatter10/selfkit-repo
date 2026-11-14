# Chrono
Status: desktop tracker running in the agent (`app_sessions`, see [../../components/selfkit-agent.md](../../components/selfkit-agent.md)); no UI yet. See [PLANNING.md](../../PLANNING.md) § Apps → Chrono for scope.

## Screens
_TBD_

## Data model
- `app_sessions`: `{app, platform, start, end, tz_offset}` (UTC ms), written by the agent.
- Planned: `web_sessions` (extension), `android_usage` (Kotlin plugin), daily summaries.

## Code map
_None yet._
