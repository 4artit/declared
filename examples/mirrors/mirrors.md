# Mirrors controller

What this controller reacts to and what it does about it. Generated from the
declarations, so it cannot drift from the code — regenerate with
`cargo run --example mirrors`.

## Features

Stateless features, one per file. `handles` and `emits` are read off the rules
below, so a feature cannot react to or emit anything this table omits.

| feature | handles | emits |
|---|---|---|
| `Heating` | `DefogChanged` | `On`, `Off` |
| `Dimming` | `PowerChanged`, `GearChanged` | `On`, `Off` |

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

```mermaid
flowchart LR
    ev_PowerChanged["PowerChanged"] --> ft_Dimming["Dimming"]
    ft_Dimming["Dimming"] -->|"DIM_ON<br/>PowerOn && !GearReverse"| ac_Dimming_On["On"]
    ft_Dimming["Dimming"] -->|"DIM_OFF<br/>else"| ac_Dimming_Off["Off"]
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

No feature reacts to this event.

### FoldPositionChanged

No feature reacts to this event.

### UserChanged

No feature reacts to this event.


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
| Guard names used by two node types | [] |
| Rule ids used twice | [] |
