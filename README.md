# Solar Hub

A personal macOS desktop app for a home solar setup: it shows what the inverter
and battery are doing right now, and uses Gemini to decide when the house
should run on battery (SBG) and when it should be in Solar mode, day and night.

Built with Tauri 2 (Rust) + React 19 + TypeScript. The inverter is read from
its cloud (Solar of Things / siseli), and the battery is read directly from its
JBD BMS over Bluetooth.

## What's in the app

| Tab | What it shows |
|---|---|
| **Dashboard** | Battery %, charge/discharge rate with a readable time estimate ("15% in ~8 min · around 6:15 AM", "Full in ~2 h 50 min"), solar, house load, grid status, output-source and smart-load controls, a summary of what the automation is doing, and electricity units taken from the grid, per meter. |
| **Battery** | Pack health, cell voltages and balance, temperatures, protection flags and short trend charts from the BMS. |
| **Inverter** | Live power flows (solar in, house load, grid), output source, device summary, advanced charger/cutoff settings and every raw field from the cloud. |
| **Automation** | Tonight's plan and battery projection, the day check, the household routine it has learned, the weather outlook, recent outages, a log of every decision and why, the CSV records folder, and all automation settings. |
| **Settings** | Solar cloud account, location and Gemini API key. |

macOS notifications fire for grid off/back, low battery, high discharge,
battery temperature, BMS protection and every automation switch.

## Requirements

- macOS
- [pnpm](https://pnpm.io)
- Rust (stable) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for macOS
- Node.js 20 or newer (the automation agent in `agent/` runs on Node)
- A [Gemini API key](https://aistudio.google.com/) for automation

## Getting started

```bash
pnpm install        # installs the app and the agent (pnpm workspace)
pnpm tauri dev      # run the app in development
pnpm tauri build    # build a release .app / .dmg
```

Then, in the app:

1. **Settings** → enter the Solar cloud account (user ID, password, station
   and device ID, time zone), your location, and the Gemini API key. The
   password and API key go into the macOS keychain, never onto disk.
2. **Battery** → scan for the BMS and connect. The app reconnects to it
   automatically afterwards.
3. **Automation** → turn it on. It starts in **dry run**, so it only says what
   it *would* do.

macOS will ask once for access to your Documents folder; allow it so the
records can be saved (see [Records](#records-csv-for-excel)).

## Output modes

The inverter's output source decides which source powers the house first:

| Mode | Order | In practice |
|---|---|---|
| **Solar** | solar → grid → battery | The grid covers whatever the sun doesn't. The battery is only used when both solar and grid are gone. |
| **SBG** | solar → battery → grid | The battery covers whatever the sun doesn't. The grid is only a fallback. |
| **Utility** | grid → battery | The grid powers the house even when there's sun. |

Automation only switches between **Solar** and **SBG**.

## How the automation works

The day is split into two windows:

- **Night** runs from sunset until sunrise plus the **sunrise buffer** (1 hour
  by default). The panels can't carry the house right at sunrise, so the night
  plan keeps running until they can.
- **Day** runs from then until sunset.

The **usual night start** (21:00 by default, adjustable from 17:00 to 23:00)
is when the battery plan normally begins. Between sunset and then, the
battery can start early (see below). Smart load and the oven boost still go
by the usual start.

### Evening: starting the battery early

A 100 Ah pack loses about 15–18% an hour to the busy evening (10–20 A before
bedtime), but only about 4% an hour once the family sleeps (around 4 A).
Starting at 21:00 with a full battery can leave a lot unused by sunrise, and
battery left unused is grid units that weren't saved. So from sunset:

- With the battery below **60%**, the agent isn't asked. The house stays in
  Solar mode until the usual start.
- Otherwise the agent gets the projection and one extra number: **how much
  the battery would still hold at sunrise if it waited** for the usual start.
  If that's well above the reserve it wants, it starts the battery now. If
  not, it waits in Solar mode and checks again later in the evening.

To do this better over time it also sees **how the last 7 nights went**:

- when the battery ran, and from what SOC
- the evening and sleeping draw in amps
- the lowest and morning SOC
- whether the next day's sun still filled the battery

This comes from the same `samples/` records, so it improves as nights are
recorded.

### Night: a battery budget, not a yes/no

Instead of asking "will the battery last until morning?", the agent answers
**"how much battery can we afford to use tonight?"** and returns a **reserve**,
for example "on battery until 30%".

- The house runs on **SBG** (from the battery) until the battery reaches the
  reserve, then switches to **Solar** (grid) for the rest of the night. It's
  fine if the battery wouldn't last until sunrise; that's what the reserve is
  for.
- The agent sizes the reserve from:
  - **Your routine:** the typical load each hour over the last 7 recorded
    nights, so a busy 21:00 with the family still up doesn't scare it off
    when the house is usually quiet by 23:00.
  - **Load-shedding:** recent grid outages. More outages mean it keeps more
    backup.
  - **The coming day's sun:** tomorrow's in the evening, today's after
    midnight. If it looks as sunny as recent days when the battery filled up,
    it can go lower. If it's cloudy, it keeps more.
  - **Your expectation:** on a normal night (sunny coming day, few outages)
    the battery is used down to about the floor (~20%) by morning. It only
    keeps more for a real reason.
  - **The season:** tonight's forecast low compared with recent nights, since
    cooler nights mean less fan and AC load.
- A few minutes after switching, it measures the real draw (5 samples after a
  2.5-minute settle) and confirms or revises the plan. It re-checks through
  the night at the interval the agent asks for (15–120 min).
- The hard **floor** (default 20%) always wins: below it, automation pauses for
  the rest of the night.

### Night: oven boost

When the inverter is in Solar mode at night (the grid carries the house, no sun) and a big load shows
up, like the oven, the app covers the spike from the battery. It switches to
**SBG with smart load off for 3 minutes**, then back to **Solar**, restoring
smart load, even if the load is still high.

- The trigger is **50 A** of load, measured on the battery side (load ÷ pack
  voltage). That's about 1.4 kW on a 24 V pack. You can change it, or turn it
  off with 0, under Tuning → "Oven boost above (A)".
- It only runs while the battery is above tonight's reserve, never into the
  backup kept for load-shedding.
- At most 4 boosts a night, with at least 5 minutes between them.
- While a boost is possible, the engine checks about every 30 seconds.
- The load comes from the inverter cloud, which can lag a minute or so behind
  reality, so a boost may start partway into a short oven run.

### Day: will the battery be full by sunset?

The app projects the battery's charge at sunset from what it *could* take, not
just what it's getting this minute. For each hour until sunset it works out:

- what the panels make now, scaled by the forecast sun for that hour (or from
  the panel size, if you've entered it),
- minus your usual house load for that hour (learned from the records, as the
  median of the last 7 days),
- capped at the inverter's max charge current setting (e.g. 45 A).

So a big temporary load, like an EV soaking up the sun, doesn't read as "won't
fill": the battery may only get 10 A now, but it catches up at up to the
inverter's limit once the load stops. Then:

- If the battery will be full, **SBG** is fine.
- If it clearly won't (little or no sun), it switches to **Solar**, so the
  grid carries the house and every bit of solar goes into the battery.
- In the last hour before sunset with no sun left, it goes to Solar mode.
  From sunset the night plan takes over (see
  [Evening](#evening-starting-the-battery-early)).

SBG during the day only makes sense while the sun is actually charging the
battery. If the battery is being drained on SBG (or, in dry run, *would* be),
it switches to Solar without asking the agent:

- straight away when the sun is done for the day (sunset, or under 50 W of
  solar in the last hour before sunset),
- otherwise after two checks in a row, so a passing cloud doesn't flip it.
  It then waits an hour before reconsidering.

Otherwise the agent is only asked when the projection disagrees with the
current mode, so it can judge the borderline cases, like a passing cloud.
It's told when the load is well above usual.

**Check now** makes the next check ask the agent straight away, even if
nothing is due and the projection already agrees. The button shows
"Checking…" until the result is back.

### Smart load

- **On = smart load enabled:** heavy loads and the connections that aren't on
  the UPS are cut. This protects the battery.
- **Off = smart load disabled:** nothing is cut; everything runs.

The app sets it like this:

- **Day:** off at sunrise, so everything runs on the sun.
- **Night:** on, to protect the battery. In summer (April–August) it goes on
  right at the night start. The rest of the year the agent picks a time
  between the night start and 23:00, and 23:00 is the latest.
- **Near morning:** back off once the battery can easily reach sunrise above
  tonight's reserve *with everything running*. That's worked out from your
  heaviest usual night-time load, or 1 kW if there's no history yet. Once off,
  it stays off until the day starts, so it doesn't flip back and forth.
- **Oven boost:** off for the 3 minutes of a boost so the oven can run, then
  restored.

Each setting is applied once when the target changes. If you change it by
hand, the app leaves it alone until the target changes again.

### Safety

- **Dry run is on by default.** The engine decides, notifies ("[Dry run] Would
  switch to battery until 30% — …") and logs, but never writes to the
  inverter. In dry run it tracks the mode it *would* be in and a simulated
  battery %, so a dry-run night plays out like a real one. Turn it off (the app
  asks you to confirm) only after a few dry-run nights look right. Turning dry
  run on or off starts the current day or night window over, so the engine
  decides again against the real inverter right away.
- If you change the output mode yourself, automation pauses until the next
  window.
- At most two switches to battery per night, and at least 10 minutes between
  switches. Oven boosts have their own limit (4 a night, 5 minutes apart).
- It won't switch to battery, or change the daytime mode, while the grid is
  off. Switching back to Solar is always allowed.
- After three failed AI calls in a row while on battery, it switches back to
  grid rather than running the battery unattended.

### Grid detection

Grid on/off comes from the inverter's AC input voltage. There's also a rule
based on how Solar mode works (solar → grid → battery): **in Solar mode, if the
battery is carrying the house, the grid is gone.** "Carrying" means
discharging more than 1 A *and* supplying at least 60% of what the house needs
beyond solar. A small trickle while the grid does the work (the inverter
sometimes pulls a few watts) doesn't count. That rule wins over a possibly
stale voltage reading. It keeps working when load-shedding also takes the
internet down, as long as the Bluetooth link to the battery is up; the
load isn't known then, so the plain 1 A rule applies.

If the inverter cloud's power-flow request fails, the app retries once and then
reuses the last reading for up to 2 minutes, marked as stale, instead of going
blank.

## Records (CSV, for Excel)

A reading is saved every time the engine checks, and every automation
decision is saved too. Checks happen every 15 minutes by default (the Check
interval setting), more often right after a switch, and never more than once
per 5 minutes. Everything is appended to monthly CSV files that are never
deleted. A shorter check interval gives finer records, for example for
catching short outages.

```
~/Documents/Solar Hub/
├── samples/2026-09.csv     timestamp, soc_pct, load_w, pv_w, battery_a, battery_v,
│                           grid_on, grid_basis, mode, source
├── decisions/2026-09.csv   timestamp, window, mode, reserve_soc, recheck_minutes,
│                           confidence, dry_run, applied, reason
└── energy/
    ├── 2026-09.csv         date, grid_units, meter1_units, meter2_units,
    │                       unassigned_units, house_kwh, solar_kwh, battery_kwh,
    │                       grid_off_minutes, data_hours
    └── meter-switches.csv  timestamp, meter
```

`energy/` holds one row per finished day (today is shown in the app but only
written once the day is over) and every changeover-switch change you record.

Timestamps are local time (`YYYY-MM-DD HH:MM:SS`). `battery_a` is positive while
charging and negative while discharging. These files are also where the agent
learns your routine and outage history from.

**Connecting Excel:** Data → Get Data → From File → **From Folder**, pick
`samples` (or `decisions`), **Combine & Load**, then **Refresh** whenever you
want the newest rows. Don't open a CSV directly and save over it; Excel can
rewrite the format while the app is still adding rows.

## Electricity units

The Dashboard's **Electricity units** card counts the units (1 unit = 1 kWh)
the house takes from the grid. It doesn't use the app's own readings. It uses
the inverter's own history from the Solar of Things cloud (the log behind the
portal's Data Analysis tab): a reading every 5 minutes, or every minute or two
while the app is open. So the count has no gaps when the Mac sleeps, and it
can fill in past days.

For each reading:

- solar covers the house first, then the battery's discharge
- only the rest counts as units, and only while the grid is up (AC input
  ≥ 100 V)
- during an outage nothing is counted

Each reading stands for the time until the next one, up to 10 minutes, so a
gap in the log isn't counted as hours of the same load. The card says when a
bill period's log has gaps.

**The inverter's own draw.** The inverter takes a little power from the grid
whenever the grid is up, and its load reading doesn't show it. The card adds a
fixed number of watts for every hour the grid was on. The default is 11 W,
fitted from the 23 Aug – 23 Sep bills: 135 units billed, 131 counted from the
log over 438 grid-on hours. Adjust it under **Inverter's own draw** until the
card matches your meters. The CSV records stay uncorrected, so changing it
updates every total.

**Two meters.** The inverter's grid input is on a changeover switch between
two meters, and the inverter can't tell which one is selected. Whenever you
flip the switch, tap **Grid from: Meter 1 | Meter 2** on the card. If you
forget, you can enter an earlier time today. Units count to whichever meter
was selected at the time. Days from before your first tap show as **Not
assigned**.

**Bill periods.** Set the day and time your meters are read, from the time
stamp on the meter photo printed on the bill ("Meters read on the 23rd at
11:57", which is also the default). On the reading day, units before that
minute go to the old bill and units after it go to the new one, worked out
from that day's inverter log.
The card shows:

- this bill so far for each meter, and what it's on pace for, with the rest
  of the period going to the meter selected now
- the last bill's totals, to compare with the actual bills

On first start the app fills in both periods from the cloud's history, one
day per second, in the background.

**What it can't see:**

- loads wired directly to a meter rather than through the inverter
- grid charging of the battery: the inverter's charging-current reading
  shows 3–4 A even when the BMS reads no current, so it isn't used

Compare with the real meters now and then. Note a reading and check it
against the app's number for the same day.

## Weather

The forecast comes from [Open-Meteo](https://open-meteo.com/). It's free and
needs no key for non-commercial use (under 10,000 calls a day, 5,000 an hour
and 600 a minute). The app fetches it every 30 minutes (about 48 calls a day)
and shows the CC BY 4.0 credit the licence requires. If the forecast is
unavailable, the agent is told so and keeps a larger reserve. A commercial
release would need Open-Meteo's paid plan.

## Project structure

```
src/                    React frontend
  pages/                one file per tab (all but Dashboard load on demand)
  components/           automation/, battery/, dashboard/, inverter/, ui/
  hooks/                data hooks (TanStack Query + Tauri events)
  lib/                  types, Tauri command wrappers, formatting, battery ETA
src-tauri/src/          Rust backend
  inverter/             Solar cloud client (live state, settings, history), grid
                        detection
  energy/               electricity units: history → units per day and meter,
                        bill periods, energy CSV
  battery/              BLE / JBD BMS connection and parsing
  automation/           engine (runner.rs), history (CSV), weather, telemetry,
                        guardrails, agent bridge
  notifications.rs      alert rules and macOS notifications
agent/                  Node scripts that call Gemini: night.js, day.js, sun.js,
                        and system.js (the one shared system prompt: modes,
                        smart load, household rules, goals)
```

Every agent call sends the same system prompt from `agent/system.js`. It holds
the house rules: what Solar/SBG/Utility and smart load mean, load-shedding,
the EV and oven, and the goals in order. The night and day prompts only add
the numbers for that moment. Change house facts in that one file.

The Rust engine runs the whole state machine and pre-computes everything the
model needs: the battery trajectory, routine, outages, forecast summary and
sunset projection. The model reasons rather than doing arithmetic. Rust calls
the Node scripts on demand and passes JSON over stdin/stdout.

## Development

```bash
cd src-tauri && cargo test          # engine, history, weather, grid, telemetry tests
pnpm exec tsc --noEmit              # typecheck the frontend
pnpm build                          # production frontend build
```

To try a prompt by hand, put `GEMINI_API_KEY=…` in `agent/.env` (it's
git-ignored). Then pipe an input in:

```bash
cd agent && pnpm night < night-input.json
```

The app itself doesn't use `agent/.env`; it passes the key from the keychain.

The app log is at `~/Library/Logs/com.mohtashimali.solarhub/solar-hub.log`.
Every agent call's stderr is logged there, including the model's response
time.

## Notes

### Battery current comes only from the BMS

There is no current sensor between the battery and the inverter, so charge and
discharge amps are only ever read from the Bluetooth BMS. Without the BMS,
current is unknown rather than guessed. SOC and capacity can still fall back
to the inverter's own fields. The day check and the battery grid rule both
need the BMS connected.

### Known limitations

- The Rust side finds `agent/` through a path fixed at build time. A built
  `.app` works on the machine it was built on, but bundling the agent and Node
  for other machines isn't done yet.
- Records, routine learning and outage detection only happen while the app is
  running. If the Mac sleeps, that's a gap in the data. (Electricity units are
  the exception: they come from the inverter's own history.)
