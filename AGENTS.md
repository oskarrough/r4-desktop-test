## Reference

[spotifast](https://github.com/crmne/spotifast) is a music player on our stack (Rust, egui, fastframe). Look there first for how to build a player UI. Clone it to `thirdparty/spotifast` (gitignored): `git clone --depth 1 https://github.com/crmne/spotifast thirdparty/spotifast`.

## Autobot models

- autobot (main thread): claude-code claude-sonnet-5-5, medium
- fix: claude-code claude-opus-5-5, medium
- verify: codex gpt-6.1-sol, medium

## Checks

`scripts/check` is the repo's checks (fmt, clippy, tests). It records a pass against the exact worktree content and toolchain, and on the same state prints that pass instead of rerunning: that counts as the checks having run. Any edit or rebase reruns everything. Don't run fastframe's own tests; it's pinned by tag. A change nobody can see (media keys, logging, window memory) needs no screenshots and no baseline build.

## Debugging (Linux)

Each run logs mpv's warnings, track loads and failures, and network errors to stderr and `radio4000.log` in the config folder's `radio4000/desktop/` (fresh each run, none in demo mode; panics append to `panic.log` beside it): `tail -f` it while debugging, `RUST_LOG=debug` for more.

Several checkouts test at once on Oskar's desktop, and a second copy of the app hands its launch to the first and exits. So run the app with `scripts/try`, never bare `cargo run`: `scripts/try start` builds this checkout and runs it as its own copy, with its own MPRIS name and its own config, cache and log under `target/try/xdg` (kept between starts, so close and reopen tests work; `scripts/try reset` forgets them). It prints the pid, the `playerctl -p` name and the log path. `scripts/try key Down`, `float [w h]`, `shot <file.png>` and `close` act on that copy's window by pid; never target `class:Radio4000` or `pkill` the app, other runs share both. Synthetic clicks don't reach the app, so drive it by keyboard (Up, Down, Return, space). Prove the real surface the change touches before calling it done: `playerctl` for media controls, `pactl list sink-inputs` for audio, a close and `start` again for window memory. `scripts/try start <youtube url>` plays audio only.

`RADIO4000_DEMO=list|tracks|info|now|volume|idle scripts/try start` opens on that view with fixed channels and a track stopped at 1:23, no network but the one avatar; `n` and `p` move the playing track so the glide can be watched: use it for screenshots, and zoom in (`magick in.png -filter point -resize 400% out.png`) to check edges against the 4px grid.
