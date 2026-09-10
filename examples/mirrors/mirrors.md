# Mirrors controller

What this controller reacts to and what it does about it. Generated from the
declarations, so it cannot drift from the code — regenerate with
`cargo run --example mirrors`.

## Features

One per file. `handles` and `emits` are read off the rules below, so a feature
cannot react to or emit anything this table omits.

| feature | handles | emits |
|---|---|---|
| `Heating` | `DefogChanged` | `On`, `Off` |
| `Dimming` | `PowerChanged`, `GearChanged` | `On`, `Off` |
| `Fold` | `PowerChanged`, `SpeedChanged` | `Fold`, `Unfold` |

## Rules

One line per `input -> output` rule. Within a feature the order is priority: the
first rule whose guard holds is the one that runs, so a rule with no guard is a
fallback.

| feature | rule | when | guard | emits |
|---|---|---|---|---|
| `Heating` | `HEAT_ON` | `DefogChanged` | `DefogOn` | `On` |
| `Heating` | `HEAT_OFF` | `DefogChanged` | else | `Off` |
| `Dimming` | `DIM_ON` | `PowerChanged`, `GearChanged` | `PowerOn && !GearReverse` | `On` |
| `Dimming` | `DIM_OFF` | `PowerChanged`, `GearChanged` | else | `Off` |
| `Fold` | `FOLD_ON_POWER_OFF` | `PowerChanged` | `PowerOff && SpeedAllowsFold && AtUnfolded` | `Fold` |
| `Fold` | `UNFOLD_ON_POWER_ON` | `PowerChanged` | `PowerOn && AtFolded` | `Unfold` |
| `Fold` | `UNFOLD_ON_SPEED` | `SpeedChanged` | `SpeedForcesUnfold && AtFolded` | `Unfold` |

## By event

One signal at a time, which is the shape a requirement is written in. A rule
covering several kinds appears under each of them; the `when` column is what
says so.

### DefogChanged

| feature | rule | when | guard | emits |
|---|---|---|---|---|
| `Heating` | `HEAT_ON` | `DefogChanged` | `DefogOn` | `On` |
| `Heating` | `HEAT_OFF` | `DefogChanged` | else | `Off` |

```mermaid
flowchart LR
    ev_DefogChanged["DefogChanged"] --> ft_Heating["Heating"]
    ft_Heating["Heating"] -->|"HEAT_ON<br/>DefogOn"| ac_Heating_On["On"]
    ft_Heating["Heating"] -->|"HEAT_OFF<br/>else"| ac_Heating_Off["Off"]
```

### PowerChanged

| feature | rule | when | guard | emits |
|---|---|---|---|---|
| `Dimming` | `DIM_ON` | `PowerChanged`, `GearChanged` | `PowerOn && !GearReverse` | `On` |
| `Dimming` | `DIM_OFF` | `PowerChanged`, `GearChanged` | else | `Off` |
| `Fold` | `FOLD_ON_POWER_OFF` | `PowerChanged` | `PowerOff && SpeedAllowsFold && AtUnfolded` | `Fold` |
| `Fold` | `UNFOLD_ON_POWER_ON` | `PowerChanged` | `PowerOn && AtFolded` | `Unfold` |

```mermaid
flowchart LR
    ev_PowerChanged["PowerChanged"] --> ft_Dimming["Dimming"]
    ft_Dimming["Dimming"] -->|"DIM_ON<br/>PowerOn && !GearReverse"| ac_Dimming_On["On"]
    ft_Dimming["Dimming"] -->|"DIM_OFF<br/>else"| ac_Dimming_Off["Off"]
    ev_PowerChanged["PowerChanged"] --> ft_Fold["Fold"]
    ft_Fold["Fold"] -->|"FOLD_ON_POWER_OFF<br/>PowerOff && SpeedAllowsFold && AtUnfolded"| ac_Fold_Fold["Fold"]
    ft_Fold["Fold"] -->|"UNFOLD_ON_POWER_ON<br/>PowerOn && AtFolded"| ac_Fold_Unfold["Unfold"]
```

### GearChanged

| feature | rule | when | guard | emits |
|---|---|---|---|---|
| `Dimming` | `DIM_ON` | `PowerChanged`, `GearChanged` | `PowerOn && !GearReverse` | `On` |
| `Dimming` | `DIM_OFF` | `PowerChanged`, `GearChanged` | else | `Off` |

```mermaid
flowchart LR
    ev_GearChanged["GearChanged"] --> ft_Dimming["Dimming"]
    ft_Dimming["Dimming"] -->|"DIM_ON<br/>PowerOn && !GearReverse"| ac_Dimming_On["On"]
    ft_Dimming["Dimming"] -->|"DIM_OFF<br/>else"| ac_Dimming_Off["Off"]
```

### SpeedChanged

| feature | rule | when | guard | emits |
|---|---|---|---|---|
| `Fold` | `UNFOLD_ON_SPEED` | `SpeedChanged` | `SpeedForcesUnfold && AtFolded` | `Unfold` |

```mermaid
flowchart LR
    ev_SpeedChanged["SpeedChanged"] --> ft_Fold["Fold"]
    ft_Fold["Fold"] -->|"UNFOLD_ON_SPEED<br/>SpeedForcesUnfold && AtFolded"| ac_Fold_Unfold["Unfold"]
```

### FoldPositionChanged

Nothing reacts to this event.

### UserChanged

Nothing reacts to this event.


## Checks

| Check | Result |
|---|---|
| Events nothing handles | [FoldPositionChanged, UserChanged] |
| Guard names used by two node types | [] |
| Rule ids used twice | [] |
