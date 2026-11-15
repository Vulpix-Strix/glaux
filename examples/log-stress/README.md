# Log Stress

Logs a fixed number of messages on every tick, to see what logging costs a
running cartridge with the log panel (F12) closed and open.

## Controls

| Virtual button | Desktop key | Action                                                     |
|----------------|-------------|------------------------------------------------------------|
| Up / Down      | Arrow keys  | Change the rate: 0, 10, 100, 1000 or 10000 messages a tick |
| A (hold)       | `Z`         | Also log a warning on every tick                           |

## Running it

```sh
mise run sync-examples -- log-stress
mise run glaux
```

Then type `load log-stress` and `run` in the Glaux shell.
