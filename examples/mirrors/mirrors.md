# Mirrors controller

What this controller reacts to and what it does about it. Generated from the
declarations, so it cannot drift from the code — regenerate with
`cargo run --example mirrors`.

## Features

Stateless features, one per file. `handles` and `emits` are read off the rules
below, so a feature cannot react to or emit anything this table omits.

| feature | handles | emits |
|---|---|---|
| `Heating` | `DefogChanged` | `HeatingOn`, `HeatingOff` |
| `Dimming` | `PowerChanged`, `GearChanged` | `DimmingOn`, `DimmingOff` |

## Rules

One line per `input -> output` rule. Within a feature the order is priority: the
first rule whose guard holds is the one that runs, so a rule with no guard is a
fallback.

| feature | when | guard | emits |
|---|---|---|---|
| `Heating` | `DefogChanged` | `DefogOn` | `HeatingOn` |
| `Heating` | `DefogChanged` | else | `HeatingOff` |
| `Dimming` | `PowerChanged`, `GearChanged` | `PowerOn && !GearReverse` | `DimmingOn` |
| `Dimming` | `PowerChanged`, `GearChanged` | else | `DimmingOff` |

## Events, features and actions

```mermaid
flowchart LR
    ev_DefogChanged["DefogChanged"] --> ft_Heating["Heating"]
    ft_Heating["Heating"] -->|"DefogOn"| ac_HeatingOn["HeatingOn"]
    ft_Heating["Heating"] -->|else| ac_HeatingOff["HeatingOff"]
    ev_PowerChanged["PowerChanged"] --> ft_Dimming["Dimming"]
    ev_GearChanged["GearChanged"] --> ft_Dimming["Dimming"]
    ft_Dimming["Dimming"] -->|"PowerOn && !GearReverse"| ac_DimmingOn["DimmingOn"]
    ft_Dimming["Dimming"] -->|else| ac_DimmingOff["DimmingOff"]
```

## Folding

Folding and unfolding are observable states, so this one is a state machine.

```mermaid
stateDiagram-v2
    [*] --> Unfolded
    Folding : Folding<br/>entry / Fold
    Unfolding : Unfolding<br/>entry / Unfold
    Unfolded --> Folding: PowerChanged<br/>[PowerOff && SpeedAllowsFold]
    Folding --> Folded: FoldPositionChanged<br/>[AtFolded]
    Folded --> Unfolding: PowerChanged<br/>[PowerOn]
    Folded --> Unfolding: SpeedChanged<br/>[SpeedForcesUnfold]
    Unfolding --> Unfolded: FoldPositionChanged<br/>[AtUnfolded]
```

## Checks

| Check | Result |
|---|---|
| Events nothing handles | [UserChanged] |
| Holes in the fold table | [] |
| Fold table is clean | true |
