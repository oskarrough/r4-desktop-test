## Reference

[spotifast](https://github.com/crmne/spotifast) is a music player on our stack (Rust, egui, fastframe). Look there first for how to build a player UI. Clone it to `thirdparty/spotifast` (gitignored): `git clone --depth 1 https://github.com/crmne/spotifast thirdparty/spotifast`.

## Autobot models

- autobot (main thread): claude-code claude-sonnet-5-5, medium
- fix: claude-code claude-opus-5-5, medium
- verify: codex gpt-6.1-sol, medium
