# Rear camera

Shows the camera in reverse, and chimes if a door opens while reversing.

## Rules

Within a feature, the first rule whose guard holds runs.

| feature | rule | when | guard | emits |
|---|---|---|---|---|
| `Camera` | `CAM_SHOW` | `GearChanged` | `InReverse` | `Show` |
| `Camera` | `CAM_HIDE` | `GearChanged` | else | `Hide` |
| `Chime` | `CHIME_DOOR_IN_REVERSE` | `GearChanged`, `DoorOpened` | `InReverse && DoorOpen` | `Warn` |

## Camera on [GearChanged]

```mermaid
flowchart LR
    ev_GearChanged["GearChanged"] --> ft_Camera["Camera"]
    ft_Camera["Camera"] -->|"CAM_SHOW<br/>InReverse"| ac_Camera_CAM_SHOW["Show"]
    ft_Camera["Camera"] -->|"CAM_HIDE<br/>else"| ac_Camera_CAM_HIDE["Hide"]
```

## Chime on [GearChanged, DoorOpened]

```mermaid
flowchart LR
    ev_GearChanged["GearChanged"] --> ft_Chime["Chime"]
    ev_DoorOpened["DoorOpened"] --> ft_Chime["Chime"]
    ft_Chime["Chime"] -->|"CHIME_DOOR_IN_REVERSE<br/>InReverse && DoorOpen"| ac_Chime_CHIME_DOOR_IN_REVERSE["Warn"]
```

## Checks

| Check | Result |
|---|---|
| Events nothing handles | [] |
| Guard names used by two node types | [] |
| Rules with no event or no action | [] |
| Camera actions never emitted | [] |
| Chime actions never emitted | [] |
