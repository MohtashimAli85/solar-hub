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
| **Dashboard** | Battery %, charge/discharge rate with a readable time estimate ("15% in ~8 min · around 6:15 AM", "Full in ~2 h 50 min"), solar, house load, grid status, output-source and smart-load controls, a summary of what the automation is doing, the solar loads (water tank, geyser, scooter: what to switch on or off now), and electricity units taken from the grid, per meter. |
| **Battery** | Pack health, cell voltages and balance, temperatures, protection flags and short trend charts from the BMS. |
| **Inverter** | Live power flows (solar in, house load, grid), output source, device summary, advanced charger/cutoff settings and every raw field from the cloud. |
| **Automation** | Tonight's plan and battery projection, the day rule, the household routine it has learned, the weather outlook, recent outages, a log of every decision and why, the CSV records folder, and all automation settings, including the water tank, geyser (and its ESP32 remote) and scooter. |
| **Settings** | Solar cloud account, location, and the Groq and Gemini API keys (both needed; Groq is tried first). |

macOS notifications fire for grid off/back, low battery, high discharge,
battery temperature, BMS protection and every automation switch.

## Requirements

- macOS
- [pnpm](https://pnpm.io)
- Rust (stable) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for macOS
- Node.js 20 or newer (the automation agent in `agent/` runs on Node)
- A [Groq API key](https://console.groq.com/keys) and a [Gemini API key](https://aistudio.google.com/) for automation

## Getting started

```bash
pnpm install        # installs the app and the agent (pnpm workspace)
pnpm tauri dev      # run the app in development
pnpm tauri build    # build a release .app / .dmg
```

Then, in the app:

1. **Settings** → enter the Solar cloud account (user ID, password, station
   and device ID, time zone), your location, and the Groq and Gemini API
   keys. The password and API keys go into the macOS keychain, never onto
   disk.
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
battery can start early (see below). The oven boost still goes by the usual
start.

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
- The app double-checks that choice. If the agent picks an early start but
  waiting would leave no more than the reserve it asked for at sunrise,
  starting early gains nothing. The app then stays in Solar mode and asks
  again at the agent's recheck time.

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

### Day: SBG unless there's no sun

By day the inverter stays on **SBG**, so the sun and the battery carry the
house. On SBG a new load (oven, kettle, EV) is first taken from the battery
while the panels ramp up, and a passing cloud does the same, so short drains
never switch the mode. Fixed rules settle the clear cases; the AI only judges
the unclear middle.

**The rules decide (no AI call):**

- **Sun stays on SBG.** PV of 300 W or more, or low PV with the battery *not*
  draining. With a full battery the panels are throttled down to the load, so
  PV can read 100 W at noon.
- **Dark now and a dark forecast → Solar** after 15 minutes. "Dark" is PV under
  150 W with the battery draining.
- **Grey (150–300 W, draining) with sun forecast** stays on SBG. Grey with a
  dark forecast goes to Solar after 45 minutes.
- **On Solar by day it switches back to SBG** once PV has held 300 W for 15
  minutes.
- **In the last hour before sunset** with no sun left (under 50 W), it goes
  to Solar mode. From sunset the night plan takes over (see
  [Evening](#evening-starting-the-battery-early)).

**The AI decides the unclear middle:** grey with a mixed forecast, dark with a
sunny or mixed forecast, or a dim sky with no forecast. It is asked once the
sky has been dim for 15 minutes, and then again at the recheck time it chose
(15–60 minutes). It gets the last 90 minutes of PV, load and battery current
every 5 minutes, the forecast until sunset, how recent days charged the
battery, the projected charge at sunset, what tonight usually needs, and
today's switches. While the sky is dim on SBG the app checks every 5 minutes so
that history is dense.

**Limits the AI can't override:**

- No switch on a dim spell shorter than 15 minutes.
- No reversing within 30 minutes of the last switch.
- At most 4 switches a day.
- Changing the mode yourself pauses automation until tonight.
- If the AI fails, the fixed rule above (dark 15 / 45 / 90 minutes depending on
  the forecast) decides instead.

The Automation tab says what it's waiting for, for example "Little sun
(60 W) and the battery is draining — staying on SBG; checking again at 11:45
if it holds". AI decisions show the model that made them. The **Today** card
still shows the projected charge at sunset.

### Smart load

- **On = smart load enabled:** heavy loads and the connections that aren't on
  the UPS are cut. This protects the battery. It only cuts anything while the
  battery is powering the house; on grid or solar nothing is cut either way.
- **Off = smart load disabled:** nothing is cut; everything runs.

The app sets it like this:

- **Day:** off at sunrise, so everything runs on the sun.
- **Night:** on only when the battery drain stays high — 25 A or more for
  10 minutes by default (both in Settings), such as AC running from the
  battery on an SBG night or through load-shedding. It has to be high for 8 of
  those 10 minutes, so a few minutes of oven or kettle don't count, and a
  light night (winter, or just lights and fans) never turns it on. While the
  battery carries the house the drain is checked every minute. Once on, it
  stays on until the near-morning release below, because with heavy loads cut
  the drain looks low and can't tell us they're safe to bring back.
- **Near morning:** back off once the battery can easily reach sunrise above
  tonight's reserve *with everything running*. That's worked out from your
  heaviest usual night-time load, or 1 kW if there's no history yet. Once off,
  it stays off until the day starts, so it doesn't flip back and forth.
- **Oven boost:** off for the 3 minutes of a boost so the oven can run, then
  restored.

Each setting is applied once when the target changes. If you change it by
hand, the app leaves it alone until the target changes again.

### Solar loads: water tank, geyser, hotplate and scooter

Four loads on the inverter's smart-load output are run on spare sun. You
switch them by hand when the app says so, except the geyser once an ESP32 is
set up to press its remote's buttons.

**The AI plans, the app checks.** In the day the AI (`agent/loads.js`) is asked
whenever the house load moves by 300 W or more, a button is pressed, a question
is answered or the scooter's % changes, and at least every 15 minutes. It gets:

- the last 90 minutes of load readings
- the usual house load and the forecast for the rest of the day
- what the app thinks is on
- each load's rules
- the next three days

It answers with:

- what is running and since when, read from the jumps in the load
- one run per load for today
- whether the scooter goes first
- the line each row shows

Before any of it is used, the app checks it:

- The loads it says are on must add up to the measured load, with the usual
  house load as the rest.
- Each load uses one of its real levels and stays inside its hours, and only on
  days it may run.
- Simulated with the forecast, the plan keeps the battery above the day floor
  and leaves it as full at sunset as the sun alone would.

An answer that fails goes back to the AI once with the exact problems. If the
second answer fails too, the app's own plan (below) runs until the next ask, and
the card says so. When the AI can't tell two loads apart (the geyser at 1200 W
or the hotplate), it asks a yes/no question on the card and in a notification;
the answer goes into what it's told next time. Without a Groq or Gemini key the
app's own plan runs as before.

| Load | Power | Rule |
|---|---|---|
| Water-tank motor | 1000 W, about 30 min | Once a day, inside its window (see the water-tank settings) |
| Geyser | 800, 1200 or 2000 W (both elements) | Heats in the sunniest stretch, 10 AM–4 PM by default; off from sunset |
| Hotplate (electric cooker) | 100–2200 W, planned at 1800 W for 2.5 h | Every other day, 10 AM–3 PM; gas when the sun is short |
| Scooter (Yadea T5L) | 600 W charger, 2.16 kWh pack | Solar only, never the grid; charges to 100% |

**What counts as spare.** Every few minutes the app works out the rest of the
day in 15-minute steps: forecast solar minus the usual house load for that
hour. A load may run as long as the battery is still **back to 100% by
sunset**. In the day a load may draw the battery down to **80%** (a setting in
the hotplate settings), but only charge that the afternoon sun puts back. Within those limits each load starts as early
as it can, so with a full battery the scooter can go on right away even while
the tank fills. On a cloudy day that means the geyser doesn't run, and you get
one note saying so.

**How much the panels really give.** Forecast solar is radiation × panel size ×
a factor learned from the last 7 days: what the panels gave while the battery
was still charging, compared with the forecast radiation for that hour. A
reading where the battery was charging at its limit counts as a floor. The
factor can go up to 2, because in winter tilted panels catch more than the
flat-ground radiation the forecast gives. Until there are 12 such readings it
uses 0.75. The night planner, the sunset projection and the water reminder all
use the same factor.

**Who goes first.** The tank first. Then the scooter, if the AI says it has to
charge today. Then the hotplate and the geyser, then the scooter if the sun is
spare. The hotplate goes ahead of the geyser, except on Fridays (everyone
bathes) and when last night dropped below 10°C; both are geyser settings. The
geyser gets the highest power the spare sun can carry, and is told to step
down to a lower power or switch off when a cloud comes and stays for 10
minutes.

**The hotplate.** Cooking days come every other day, counted from the last day
you said yes. On a cooking day the card shows the best 2.5 hours between 10 AM
and 3 PM and the highest level the sun can carry for all of it, or says to cook
on gas. The hotplate's level changes as you cook, so the app can't tell it from
the geyser by the step alone. In the cooking hours, a load it didn't ask for
makes it ask "is it the hotplate?" (a notification, and **Yes, cooking** / **No**
on the card). Until you answer, it isn't counted as the geyser. While you cook,
the hotplate's draw is the house load less what it was before you started. If
the sun can't carry that, the card says to turn it down to a level it can. The
session ends with **Done cooking**, or once the extra load has been gone for
10 minutes.

**What's running.** The app spots each load from the step it makes in the house
load: +600 W is the scooter, +800/+1200/+2000 W the geyser, +1000 W the motor.
A load it just asked for wins a close call. When the geyser drops out on its
own, that's the thermostat: the water is hot, and the card says how many kWh
it took. When the charger drops out above about 80%, the scooter is full.

**The scooter's %.** The app reads it over Bluetooth from the scooter
(`YD64071540`, after the pairing handshake in `src-tauri/src/scooter/`). The
scooter only updates its % while switched on, so a normal read gives the % it
held when it was last switched off, and while the charger runs the app
estimates it at about 25% an hour. With **Read a live % while charging** on,
it switches the scooter on, reads, and switches it off again about once an
hour while charging. Use **Test on → read → off** in the scooter settings
first, standing next to the scooter. The scooter has to be within Bluetooth
range of the Mac.

**Is it charging?** The scooter's Bluetooth module doesn't report charging,
and the +600 W of the charger is easy to confuse with the hotplate. **Read
now** on the dashboard switches the scooter on, reads its %, and switches it
off (about 20 s). Tap it again 10–15 minutes later: a rise of 1% or more (over that read, or
over the app's last figure) means the charger is on (it adds about 4% in 10 minutes), the same % means it isn't,
and a drop means it was ridden. The row shows the result, and the AI is told
it as fact, like a Change. The app never switches the scooter on by itself for
this.

**Daily check.** Once a day at 10:30 (a scooter setting) the app switches the
scooter on, reads its %, and switches it off. If the scooter isn't near, it
tries twice more, 30 minutes apart, then says so on the card. A manual **Read
now** after 10:30 counts as the day's check, and a restart doesn't repeat it.

**Charge today?** The family's rule, which the AI has to follow:

- **At or below 80%:** charge today.
- **Above 80%:** no charge is needed, unless tomorrow's forecast leaves too
  little spare sun to charge it (after the geyser) while today's sun can.

While a charge is needed and the scooter's slot has started, "Plug in the
scooter" repeats every 30 minutes until the charger's +600 W shows up or you
tap **Plugged in**. When the charger switches itself off, the app says:
"Scooter full — switch the charger off at the wall first, then unplug the
battery" (the order the manual asks for). The first plan of the day, a change
in the scooter's turn and every fallback show in the log.

**Do this now.** A strip at the top of the dashboard lists only what needs
your hands right now, each with its button:

- something to switch on, off or down
- the scooter to plug in or unplug
- a question from the AI
- "Is the tank filled?", asked when a motor run stops

It's hidden when there's nothing to do.

**Spare sun now.** The Solar loads header says how much sun is spare right now
after the house and what's on, and which still-needed load fits and until when
("Spare now 2.2 kW · geyser 2000 W fits until about 1 PM").

**Sun today.** A line chart on the dashboard shows the panels' expected output
through today next to what they really gave so far. The forecast is the
radiation × the array size × a factor learned from the last 21 days. The first
and last hours of light learn their own factor per half hour, because low sun
and shade cut the panels far more than the flat-ground forecast says. The
forecast never goes above the panels' best reading. The last hour's readings
(only while the battery isn't full) pull the next few hours towards what the
sky is really doing. A dashed line marks 100 W, and the line above the chart
says when the panels give 100 W or more and when they peak.

**Geyser over the ESP32.** An ESP32 next to the geyser sends its IR remote
codes: `POST /press?button=power|800|1200|up|down` and `GET /status`. The
geyser is left switched on and the app never presses On/Off: 800 and 1200
each toggle one element (both is 2000 W, both off draws nothing), and the
geyser keeps its elements through a power cut. Put the ESP32's address in
the geyser settings and turn on **Switch the geyser automatically**. The app
works every press out from what the inverter sees drawing, not from memory:
with nothing drawing it presses the elements for the level, while it draws it
toggles to the new level or switches the drawing elements off. Every press
sends a notification and is checked on a reading taken after it; the geyser
row shows the last action as checking, confirmed or not confirmed. A
switch-on that still draws nothing (the water may already be hot) is not
repeated; the app says so and goes back to reminders for the day, though a
geyser seen drawing when it should be off still has its elements switched
off. When the thermostat cuts it out after a real heat-up, the app switches
the selected elements off so it doesn't heat again on its own. A geyser that
stops after only a little energy is taken as switched off by hand, not hot.
It never presses anything in dry run.

**Switching the geyser from the app.** The geyser row's ⋯ menu has **Switch
on at 800 / 1200 / 2000 W** and **Switch off** (also on the phone). They are
worked out from the load and checked like the app's own presses, even with
automatic control off; Switch off presses nothing unless the geyser is
drawing. By day, every geyser run is watched for its first 10 minutes before
the plan may change it: the app then sums up the sun, the house load, whether
the battery charged or drained and the battery %, asks the AI again with those
readings, and sends the result ("Decision: keep it on", "turn it down to
800 W" or "switch it off"; "Suggestion" when automatic control is off). At
night a geyser switched on from the app stays on until it stops drawing or
you switch it off.

**Buttons.** On the dashboard: **Done** for the tank, **Hot enough** for the
geyser, **Yes, cooking** / **No**, **Done cooking** and **Not today** for the
hotplate, **Read now** and **Not today** for the scooter. **Change** on every row
opens a short list of what could be true ("On at 1200 W", "Off, not hot yet",
"Cooked today", "Charging"…); picking one applies it at once. Use it when the
app got it wrong (e.g. the geyser was switched off by hand, not by its
thermostat; the hotplate was used). The app takes it as
fact, tells the AI, and rejects an AI answer that contradicts it until
the load changes by 400 W or an hour has passed. Reminders repeat at the
water tank's repeat interval. At night the geyser and scooter only get one
"switch off" reminder.

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
- If the BMS drops out at night, the battery % is estimated from the last BMS
  reading: on battery it drains at house load ÷ battery volts, scaled by how
  much more the BMS has measured than that over the last week (about 1.1×);
  in Solar mode it drains at the standby draw. The estimate still enforces the
  reserve and the floor, but it never starts the battery. You get one
  notification when it takes over, and switches made on it say so.

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
│                           confidence, dry_run, applied, reason, model
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
and 600 a minute). The app fetches the last 7 days and the next 4 every 30 minutes (about 48 calls a day)
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
                        guardrails, agent bridge, solar loads planner (loads.rs)
  scooter/              Yadea scooter over Bluetooth: pairing and reading its %
  geyser/               client for the ESP32 that presses the geyser's remote
  notifications.rs      alert rules and macOS notifications
esp32/geyser-remote/    ESP32 firmware (Arduino) for the geyser's IR remote
agent/                  Node scripts that call the AI models: night.js, day.js,
                        loads.js (the solar loads), sun.js, and system.js (the one shared system prompt: modes,
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

**Models.** Each call tries these models in order until one answers. Groq's
120B goes first because it answers in a few seconds, while Google's free tier
often answers 503 "high demand". Groq's 20B is fast but follows the rules
less closely, so it comes after Gemini Flash Lite. Each has its own free-tier
allowance.

| Order | Model | Free-tier limits |
|---|---|---|
| 1 | Groq `openai/gpt-oss-120b` | 1,000 a day, 8,000 tokens a minute |
| 2 | `gemini-3.5-flash-lite` | 15 a minute, 500 a day |
| 3 | `gemini-3.1-flash-lite` | 15 a minute, 500 a day |
| 4 | Groq `openai/gpt-oss-20b` | 1,000 a day, 8,000 tokens a minute |
| 5–8 | `gemini-3.8-flash`, `3.7-flash`, `3.6-flash`, `3.5-flash` | 5 a minute, 20 a day each |
| 9 | `gemma-4-26b-a4b-it` | 30 a minute, 14,400 a day |

An overloaded model fails in a second or two. Each Gemini attempt gets up to
12 seconds, and Gemma always keeps 20 seconds in reserve. Gemma is last
because it keeps answering when Gemini is busy, but its reasons read worse.
Everything fits in 50 seconds. Gemini 2.5 Flash and 2.5 Flash Lite still
show on the limits page, but new API keys get 404 "no longer available to
new users". The list is `MODELS` in `agent/lib.js`; each provider (Gemini,
Groq) is one small function there. Both keys are needed; they're entered in
Settings and kept in the macOS Keychain, and automation shows "Set a Groq API
key in Settings" (or Gemini) until both are saved. A bad key or rejected
request skips the rest of that provider's models, not the other provider's.

Every decision records **which model made it**. The Automation tab shows it
under each decision ("Decided by Gemini 3.1 Flash Lite", or "the app's own
rules" for the engine's fixed rules), and the `decisions` CSV has a `model`
column. If every model fails, the app shows why ("Gemini is busy or down",
"no internet connection", …) and tries again at the next check.

## Development

```bash
cd src-tauri && cargo test          # engine, history, weather, grid, telemetry tests
pnpm exec tsc --noEmit              # typecheck the frontend
pnpm build                          # production frontend build
```

To try a prompt by hand, put `GROQ_API_KEY=…` and `GEMINI_API_KEY=…` in `agent/.env` (it's
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
to the inverter's own fields. The battery grid rule needs the BMS connected.
Without it, the day rule judges "battery draining" from house load minus
solar instead.

### Known limitations

- The Rust side finds `agent/` through a path fixed at build time. A built
  `.app` works on the machine it was built on, but bundling the agent and Node
  for other machines isn't done yet.
- Records, routine learning and outage detection only happen while the app is
  running. If the Mac sleeps, that's a gap in the data. (Electricity units are
  the exception: they come from the inverter's own history.)
