## Reference

[spotifast](https://github.com/crmne/spotifast) is a music player on our stack (Rust, egui, fastframe). Look there first for how to build a player UI. Clone it to `thirdparty/spotifast` (gitignored): `git clone --depth 1 https://github.com/crmne/spotifast thirdparty/spotifast`.

## Autobot models

- autobot (main thread): claude-code claude-sonnet-5-5, medium
- fix: claude-code claude-opus-5-5, medium
- verify: codex gpt-6.1-sol, medium

## Debugging (Linux)

Run `timeout 20 cargo run` (or `-- <youtube url>` for audio only) and prove it works: `pactl list sink-inputs | grep -i radio4000` shows real audio, `grim -g "$(hyprctl clients -j | jq -r '.[] | select(.class=="Radio4000") | "\(.at[0]),\(.at[1]) \(.size[0])x\(.size[1])"')" /tmp/r4.png` screenshots the window to look at.
Synthetic clicks don't reach the app, so drive it by keyboard (Tab, Return, Space). Stop it with `pkill -x radio4000`; `pkill -f` kills your own shell.
