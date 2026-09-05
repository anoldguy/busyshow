# busybody

Put an animated GIF, WebP or APNG on a [BUSY Bar](https://busy.app/).

```console
cargo install busybody --locked

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

Prebuilt binaries for Linux, macOS and Windows are attached to each
[GitHub release](https://github.com/anoldguy/busybody/releases).

## The library

The conversion and the `.anim` container live in
[`busybar-anim`](crates/busybar-anim/README.md), a separate crate in this
repository. `busybody` is its reference consumer: if you want `.anim` files
from your own program rather than from a shell, depend on the crate instead.

```toml
[dependencies]
busybar-anim = "0.1.0" # check latest version https://crates.io/crates/busybar-anim
```

# License

Licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
