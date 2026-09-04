# busybody

Convert GIF, animated WebP and APNG files into the BUSY Bar's `.anim` format,
and put them on the bar.

```console
cargo install busybody

busybody convert bounce.gif                 # writes bounce.anim, sized for the front display
busybody convert bounce.gif --screen back   # 160x80, 16-level grey
busybody show bounce.gif --seconds 10       # convert, upload, and play it
```

`show` talks to the bar over USB by default. Over Wi-Fi pass `--url http://<ip>`
and the local API password with `--api-token`, or set `BUSYBAR_URL` and
`BUSYBAR_API_TOKEN`. The flags match the [`busybar`](https://crates.io/crates/busybar)
CLI, which handles everything else the bar can do.

Frames are scaled to cover the target display and centre cropped. Timing comes
from the file's own frame delays.

## Library

```rust
let anim = busybody::convert(&std::fs::read("bounce.gif")?, busybody::Target::FRONT)?;
```

`busybody::encode` takes frames you already have. Turn off the `cli` feature to
skip the HTTP dependencies.

## Format

The `.anim` container ("bicycle0") is described in `lib/anim_file/anim_file_format.h`
of the [firmware](https://github.com/busy-app/busybar-firmware). The encoder here
is byte-identical to the reference `scripts/seq2anim.py`, which the golden tests
under `tests/fixtures/golden` pin down.
